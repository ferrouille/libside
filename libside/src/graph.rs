use crate::requirements::{Requirement, Supports};
use crate::system::System;
use serde::{Deserialize, Serialize};
use std::fmt::Display;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GraphNode<R> {
    requirement: R,
    preconditions: Vec<usize>,
    pre_existing: bool,
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub struct GraphNodeReference(usize);

#[derive(Debug, Copy, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pending;

#[derive(Debug, Copy, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Applied;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Graph<R, State> {
    nodes: Vec<GraphNode<R>>,
    state: State,
}

impl<R: Requirement> Graph<R, Pending> {
    /// Adds a new node to the graph
    pub fn add<'a, T>(
        &mut self,
        requirement: T,
        depends_on: impl IntoIterator<Item = &'a GraphNodeReference>,
    ) -> GraphNodeReference
    where
        R: Supports<T>,
    {
        let index = self.nodes.len();
        self.nodes.push(GraphNode {
            requirement: Supports::create_from(requirement),
            preconditions: depends_on.into_iter().map(|r| r.0).collect(),
            pre_existing: false,
        });

        GraphNodeReference(index)
    }

    pub fn apply_execution_results(mut self, results: ApplyResult) -> Graph<R, Applied> {
        for entry in results.pre_existing {
            self.nodes[entry.0].pre_existing = true;
        }

        Graph {
            nodes: self.nodes,
            state: Applied,
        }
    }
}

impl<R: Requirement, State: Default + Copy> Graph<R, State> {
    pub fn new() -> Self {
        Graph {
            nodes: Vec::new(),
            state: State::default(),
        }
    }

    pub fn invert(&self) -> Graph<R, State> {
        Graph {
            nodes: self
                .nodes
                .iter()
                .enumerate()
                .map(|(index, n)| GraphNode {
                    requirement: n.requirement.clone(),
                    preconditions: self
                        .nodes
                        .iter()
                        .rev()
                        .enumerate()
                        .filter(|(_, m)| m.preconditions.contains(&index))
                        .map(|(index, _)| index)
                        .collect(),
                    pre_existing: n.pre_existing,
                })
                .rev()
                .collect(),
            state: self.state,
        }
    }

    fn collect_inherited_preconditions(
        &self,
        node: &GraphNode<R>,
        map: &[Option<usize>],
    ) -> Vec<usize> {
        let mut result = Vec::new();
        let mut scanlist = Vec::new();
        scanlist.push(node);

        while let Some(node) = scanlist.pop() {
            for index in node.preconditions.iter().copied() {
                match map[index] {
                    None => scanlist.push(&self.nodes[index]),
                    Some(new_index) => result.push(new_index),
                }
            }
        }

        result
    }

    pub fn retain(&mut self, f: impl Fn(usize, &GraphNode<R>) -> bool) {
        let mut mapping = vec![None; self.nodes.len()];
        let mut counter = 0;
        for (_, (_, mapping)) in self
            .nodes
            .iter()
            .zip(mapping.iter_mut())
            .enumerate()
            .filter(|(index, (node, _))| f(*index, node))
        {
            *mapping = Some(counter);
            counter += 1;
        }

        let inherited_preconditions = mapping
            .iter()
            .zip(self.nodes.iter())
            .map(|(map, node)| match map {
                Some(_) => Vec::new(),
                None => self.collect_inherited_preconditions(node, &mapping),
            })
            .collect::<Vec<_>>();

        // Remap all preconditions
        for node in self.nodes.iter_mut() {
            let mut new_preconditions = Vec::new();
            node.preconditions.retain(|pc| match mapping[*pc] {
                Some(_) => true,
                None => {
                    new_preconditions.extend(inherited_preconditions[*pc].iter().copied());
                    false
                }
            });

            for pc in node.preconditions.iter_mut() {
                *pc = mapping[*pc].unwrap();
            }

            for new in new_preconditions {
                if !node.preconditions.contains(&new) {
                    node.preconditions.push(new);
                }
            }
        }

        // Remove nodes
        for (index, _) in mapping
            .iter()
            .enumerate()
            .rev()
            .filter(|(_, k)| k.is_none())
        {
            self.nodes.remove(index);
        }
    }

    pub fn extract_undo_graph<S: System>(
        &self,
        _system: &mut S,
        prev: &Graph<R, Applied>,
    ) -> Result<Graph<R, Applied>, R::HasBeenCreatedError<S>> {
        let mut undo = prev.invert();
        let mut nodes_to_undo = vec![false; undo.nodes.len()];
        for (node, undo) in undo.nodes.iter().zip(nodes_to_undo.iter_mut()) {
            if !self
                .nodes
                .iter()
                .any(|new_node| node.requirement.affects(&new_node.requirement))
                && node.requirement.can_undo()
            {
                // This node is no longer present in the new graph, which means we need to undo whatever effect it had
                *undo = true;
            }
        }

        undo.retain(|index, _| nodes_to_undo[index]);
        Ok(undo)
    }

    pub fn compare_with<'g, S: System>(
        &'g self,
        system: &mut S,
        prev: &'g Graph<R, Applied>,
    ) -> Result<ComparedGraph<'g, R, State>, R::HasBeenCreatedError<S>> {
        let undo = self.extract_undo_graph(system, prev)?;
        Ok(ComparedGraph {
            prev,
            undo,
            target: self,
        })
    }

    pub fn generate_verify_sequence<'r>(&'r self) -> Result<VerifySequence<'r, R>, ()> {
        Ok(VerifySequence {
            items: self.nodes.iter().map(|n| &n.requirement).collect(),
        })
    }
}

impl<R: Requirement> Graph<R, Applied> {
    pub fn generate_fix_sequence<S: System>(
        &self,
        _system: &mut S,
    ) -> Result<ApplySequence<R>, ()> {
        let mut result = ApplySequence {
            undo: Vec::new(),
            todo: Vec::new(),
            prev: self,
        };

        let mut walker = GraphWalker::new(&self);
        while let Some((index, node)) = walker.next() {
            result.todo.push(Do {
                created_by_us: true,
                should_exist: true,
                source: GraphNodeReference(index),
                requirement: &node.requirement,
            });
        }

        Ok(result)
    }
}

impl<'g, R: Requirement, State> ComparedGraph<'g, R, State> {
    pub fn generate_application_sequence<S: System>(
        &self,
        _system: &mut S,
    ) -> Result<ApplySequence<R>, ()> {
        let mut result = ApplySequence {
            undo: Vec::new(),
            todo: Vec::new(),
            prev: self.prev,
        };
        let mut walker = GraphWalker::new(&self.undo);
        while let Some((_, node)) = walker.next() {
            result.undo.push(Undo {
                pre_existing: node.pre_existing,
                requirement: &node.requirement,
            });
        }

        let mut walker = GraphWalker::new(&self.target);
        while let Some((index, node)) = walker.next() {
            let (should_exist, created_by_us) = match self
                .prev
                .nodes
                .iter()
                .find(|n| n.requirement.affects(&node.requirement))
            {
                Some(prev) => (true, !prev.pre_existing),
                None => (false, false),
            };

            result.todo.push(Do {
                created_by_us,
                should_exist,
                source: GraphNodeReference(index),
                requirement: &node.requirement,
            });
        }

        Ok(result)
    }
}

pub struct ComparedGraph<'g, R, State> {
    undo: Graph<R, Applied>,
    prev: &'g Graph<R, Applied>,
    target: &'g Graph<R, State>,
}

pub struct GraphWalker<'a, R, State> {
    graph: &'a Graph<R, State>,
    fulfilled: Vec<bool>,
}

impl<'a, R, State> GraphWalker<'a, R, State> {
    pub fn new(graph: &'a Graph<R, State>) -> Self {
        GraphWalker {
            fulfilled: vec![false; graph.nodes.len()],
            graph,
        }
    }

    pub fn next(&mut self) -> Option<(usize, &'a GraphNode<R>)> {
        for (index, node) in self.graph.nodes.iter().enumerate().rev() {
            if !self.fulfilled[index] && node.preconditions.iter().all(|n| self.fulfilled[*n]) {
                self.fulfilled[index] = true;
                return Some((index, node));
            }
        }

        assert!(self.fulfilled.iter().all(|f| *f));
        None
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Undo<'r, R> {
    pre_existing: bool,
    requirement: &'r R,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Do<'r, R> {
    requirement: &'r R,
    created_by_us: bool,
    should_exist: bool,
    source: GraphNodeReference,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ApplySequence<'r, R> {
    undo: Vec<Undo<'r, R>>,
    todo: Vec<Do<'r, R>>,
    prev: &'r Graph<R, Applied>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct VerifySequence<'r, R> {
    items: Vec<&'r R>,
}

#[must_use]
#[derive(Debug)]
pub struct ApplyResult {
    pre_existing: Vec<GraphNodeReference>,
}

#[derive(Debug, thiserror::Error)]
pub enum RequirementOperationError<R: Requirement, S: System> {
    #[error("couldn't be checked: {}", inner)]
    UnableToCheck { inner: R::HasBeenCreatedError<S> },

    #[error("couldn't be created: {}", inner)]
    CreateFailed { inner: R::CreateError<S> },

    #[error("couldn't be modified: {}", inner)]
    ModifyFailed { inner: R::ModifyError<S> },

    #[error("couldn't be deleted: {}", inner)]
    DeleteFailed { inner: R::DeleteError<S> },

    #[error("already exists, refusing to overwrite")]
    PreExisting,
}

#[derive(Debug, thiserror::Error)]
#[error("{} {}", requirement, inner)]
pub struct RunError<R: Requirement, S: System> {
    requirement: R,
    pub revert_info: RevertInfo,
    inner: RequirementOperationError<R, S>,
}

#[derive(Copy, Clone, Debug)]
pub enum Position {
    Undo(usize),
    Todo(usize),
}

#[derive(Clone, Debug)]
pub struct RevertInfo {
    position: Position,
    pre_existing: Vec<GraphNodeReference>,
}

impl<'r, R: Requirement> ApplySequence<'r, R> {
    #[must_use]
    pub fn run<S: System>(
        &self,
        system: &mut S,
        ask_overwrite: impl Fn(&str) -> bool,
    ) -> Result<ApplyResult, RunError<R, S>> {
        let mut result = ApplyResult {
            pre_existing: Vec::new(),
        };

        for (index, entry) in self.undo.iter().enumerate() {
            println!("  undo: {}", entry.requirement);
            if entry.pre_existing {
                entry.requirement.pre_existing_delete(system)
            } else {
                entry.requirement.delete(system)
            }
            .map_err(|inner| RunError {
                requirement: entry.requirement.clone(),
                revert_info: RevertInfo {
                    position: Position::Undo(index),
                    pre_existing: result.pre_existing.clone(),
                },
                inner: RequirementOperationError::DeleteFailed { inner },
            })?;
        }

        for (index, entry) in self.todo.iter().enumerate() {
            let r = &entry.requirement;
            println!("  require: {}", r);
            match r.has_been_created(system) {
                Ok(has_been_created) => {
                    if has_been_created {
                        if !entry.should_exist && !r.may_pre_exist() {
                            if !ask_overwrite(&format!("{}", r)) {
                                return Err(RunError {
                                    requirement: entry.requirement.clone(),
                                    revert_info: RevertInfo {
                                        position: Position::Todo(index),
                                        pre_existing: result.pre_existing.clone(),
                                    },
                                    inner: RequirementOperationError::PreExisting,
                                });
                            }
                        }

                        if !entry.created_by_us {
                            result.pre_existing.push(entry.source);
                        }

                        r.modify(system).map_err(|inner| RunError {
                            requirement: entry.requirement.clone(),
                            revert_info: RevertInfo {
                                position: Position::Todo(index),
                                pre_existing: result.pre_existing.clone(),
                            },
                            inner: RequirementOperationError::ModifyFailed { inner },
                        })?;
                    } else {
                        r.create(system).map_err(|inner| RunError {
                            requirement: entry.requirement.clone(),
                            revert_info: RevertInfo {
                                position: Position::Todo(index),
                                pre_existing: result.pre_existing.clone(),
                            },
                            inner: RequirementOperationError::CreateFailed { inner },
                        })?;
                    }
                }
                Err(inner) => {
                    return Err(RunError {
                        requirement: entry.requirement.clone(),
                        revert_info: RevertInfo {
                            position: Position::Todo(index),
                            pre_existing: result.pre_existing.clone(),
                        },
                        inner: RequirementOperationError::UnableToCheck { inner },
                    })
                }
            }
        }

        Ok(result)
    }

    pub fn revert<S: System>(
        &self,
        system: &mut S,
        info: &RevertInfo,
    ) -> Result<(), RunError<R, S>> {
        let num_todo = match info.position {
            Position::Undo(_) => 0,
            Position::Todo(index) => index,
        };

        // We need to undo any changes that won't be overwritten by re-applying the previous graph
        for entry in self.todo.iter().take(num_todo).rev() {
            if !self
                .prev
                .nodes
                .iter()
                .any(|n| n.requirement.affects(entry.requirement))
            {
                println!("  undo: {}", entry.requirement);
                if entry.requirement.can_undo() {
                    if info.pre_existing.contains(&entry.source) {
                        entry.requirement.pre_existing_delete(system)
                    } else {
                        entry.requirement.delete(system)
                    }
                    .unwrap();
                }
            }
        }

        let fix_sequence = self.prev.generate_fix_sequence(system).unwrap();
        let _ = fix_sequence.run(system, |_| false).unwrap();

        Ok(())
    }
}

#[derive(Clone, Debug)]
pub enum VerificationState<'r, R> {
    Ok,
    Invalid { invalid: Vec<&'r R> },
}

impl<'r, R: Display> Display for VerificationState<'r, R> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VerificationState::Ok => write!(f, "all OK")?,
            VerificationState::Invalid { invalid } => {
                for item in invalid.iter() {
                    writeln!(f, "corrupted: {}", item)?;
                }
            }
        }

        Ok(())
    }
}

impl<'r, R: Requirement + Display> VerifySequence<'r, R> {
    pub fn run<S: System>(self, system: &mut S) -> Result<VerificationState<'r, R>, ()> {
        let mut invalid = Vec::new();
        for entry in self.items {
            if entry.verify(system)? {
                println!("  ok: {}", entry);
            } else {
                println!("  invalid: {}", entry);
                invalid.push(entry);
            }
        }

        Ok(if invalid.len() > 0 {
            VerificationState::Invalid { invalid }
        } else {
            VerificationState::Ok
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        graph::{Applied, ApplyResult, Do, GraphNodeReference, Pending, Undo},
        requirements::Supports,
    };
    use serde::{Deserialize, Serialize};
    use std::{collections::HashSet, fmt::Display, path::PathBuf};

    use super::{Graph, Requirement, System};

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    struct Foo {
        id: u64,
        can_undo: bool,
    }

    impl Foo {
        const ROOT: Foo = Foo {
            id: 0,
            can_undo: true,
        };
        const ROOT_NOUNDO: Foo = Foo {
            id: 0,
            can_undo: false,
        };
        const A: Foo = Foo {
            id: 1,
            can_undo: true,
        };
        const A_NOUNDO: Foo = Foo {
            id: 1,
            can_undo: false,
        };
        const B: Foo = Foo {
            id: 2,
            can_undo: true,
        };
        const B_NOUNDO: Foo = Foo {
            id: 2,
            can_undo: false,
        };
        const C: Foo = Foo {
            id: 3,
            can_undo: true,
        };
        const C_NOUNDO: Foo = Foo {
            id: 3,
            can_undo: false,
        };
        const D: Foo = Foo {
            id: 4,
            can_undo: true,
        };
        const END: Foo = Foo {
            id: 100,
            can_undo: true,
        };
    }

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    struct AlwaysFail;

    impl Display for AlwaysFail {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "AlwaysFail")
        }
    }

    impl Requirement for AlwaysFail {
        const NAME: &'static str = "alwaysfail";

        type CreateError<S: System> = FakeError;
        type ModifyError<S: System> = FakeError;
        type DeleteError<S: System> = FakeError;
        type HasBeenCreatedError<S: System> = FakeError;

        fn create<S: System>(&self, _system: &mut S) -> Result<(), Self::CreateError<S>> {
            Err(FakeError)
        }
        fn modify<S: System>(&self, _system: &mut S) -> Result<(), Self::ModifyError<S>> {
            Err(FakeError)
        }
        fn delete<S: System>(&self, _system: &mut S) -> Result<(), Self::DeleteError<S>> {
            Err(FakeError)
        }

        fn has_been_created<S: System>(
            &self,
            _system: &mut S,
        ) -> Result<bool, Self::HasBeenCreatedError<S>> {
            Err(FakeError)
        }

        fn affects(&self, _other: &Self) -> bool {
            true
        }
        fn supports_modifications(&self) -> bool {
            false
        }
        fn can_undo(&self) -> bool {
            true
        }
        fn may_pre_exist(&self) -> bool {
            false
        }
        fn verify<S: System>(&self, _system: &mut S) -> Result<bool, ()> {
            Ok(true)
        }
    }

    #[derive(Debug, thiserror::Error)]
    #[error("Error")]
    struct FakeError;

    impl Requirement for Foo {
        type CreateError<S: System> = S::Error;
        type ModifyError<S: System> = FakeError;
        type DeleteError<S: System> = S::Error;
        type HasBeenCreatedError<S: System> = S::Error;

        fn create<S: super::System>(&self, system: &mut S) -> Result<(), Self::CreateError<S>> {
            Ok(system.copy_file(&PathBuf::new(), &PathBuf::from(format!("{}", self.id)))?)
        }

        fn modify<S: super::System>(&self, _system: &mut S) -> Result<(), Self::ModifyError<S>> {
            Ok(())
        }

        fn delete<S: super::System>(&self, system: &mut S) -> Result<(), Self::DeleteError<S>> {
            Ok(system.remove_file(&PathBuf::from(format!("{}", self.id)))?)
        }

        fn has_been_created<S: super::System>(
            &self,
            system: &mut S,
        ) -> Result<bool, Self::HasBeenCreatedError<S>> {
            Ok(system.path_exists(&PathBuf::from(format!("{}", self.id)))?)
        }

        fn affects(&self, other: &Self) -> bool {
            self.id == other.id
        }

        fn supports_modifications(&self) -> bool {
            false
        }

        fn can_undo(&self) -> bool {
            self.can_undo
        }

        fn may_pre_exist(&self) -> bool {
            false
        }

        fn verify<S: System>(&self, system: &mut S) -> Result<bool, ()> {
            self.has_been_created(system).map_err(|_| ())
        }

        const NAME: &'static str = "foo";
    }

    impl std::fmt::Display for Foo {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            std::fmt::Debug::fmt(self, f)
        }
    }

    #[derive(Debug)]
    struct FakeSystem {
        created: HashSet<PathBuf>,
    }

    impl System for FakeSystem {
        type Error = FakeError;
        type CommandError = FakeError;

        fn path_exists(&self, path: &std::path::Path) -> Result<bool, Self::Error> {
            Ok(self.created.contains(&path.to_path_buf()))
        }

        fn path_is_dir(&self, _path: &std::path::Path) -> Result<bool, Self::Error> {
            todo!()
        }

        fn file_contents(&self, _path: &std::path::Path) -> Result<Vec<u8>, Self::Error> {
            todo!()
        }

        fn execute_command(
            &self,
            _path: &str,
            _args: &[&str],
        ) -> Result<crate::system::CommandResult, Self::Error> {
            todo!()
        }

        fn copy_file(
            &mut self,
            _from: &std::path::Path,
            to: &std::path::Path,
        ) -> Result<(), Self::Error> {
            self.created.insert(to.to_path_buf());

            Ok(())
        }

        fn make_dir(&mut self, path: &std::path::Path) -> Result<(), Self::Error> {
            self.created.insert(path.to_path_buf());

            Ok(())
        }

        fn remove_dir(&mut self, path: &std::path::Path) -> Result<(), Self::Error> {
            self.created.retain(|item| item != path);

            Ok(())
        }

        fn remove_file(&mut self, path: &std::path::Path) -> Result<(), Self::Error> {
            self.created.retain(|item| item != path);

            Ok(())
        }

        fn get_user(&mut self, _name: &str) -> Result<Option<()>, Self::Error> {
            todo!()
        }

        fn execute_command_with_input(
            &self,
            _path: &str,
            _args: &[&str],
            _input: &[u8],
        ) -> Result<crate::system::CommandResult, Self::CommandError> {
            todo!()
        }

        fn chmod(&mut self, _path: &std::path::Path, _mode: u32) -> Result<(), Self::Error> {
            todo!()
        }

        fn put_file_contents(
            &self,
            _path: &std::path::Path,
            _contents: &[u8],
        ) -> Result<(), Self::Error> {
            todo!()
        }

        fn make_dir_all(&mut self, _path: &std::path::Path) -> Result<(), Self::Error> {
            todo!()
        }

        fn dir_is_empty(&mut self, _path: &std::path::Path) -> Result<bool, Self::Error> {
            todo!()
        }

        fn read_dir(&mut self, _path: &std::path::Path) -> Result<Vec<String>, Self::Error> {
            todo!()
        }
    }

    #[test]
    pub fn invert() {
        let mut g = Graph::<Foo, Pending>::new();
        let root = g.add(Foo::ROOT, &[]);
        let a = g.add(Foo::A, &[root]);
        let b = g.add(Foo::B, &[root]);
        let c = g.add(Foo::C, &[a, root]);
        let _end = g.add(Foo::END, &[b, c]);

        println!("graph    : {:?}", g);

        let g = g.invert();

        println!("inv      : {:?}", g);

        let mut expected = Graph::<Foo, Pending>::new();
        let end = expected.add(Foo::END, &[]);
        let c = expected.add(Foo::C, &[end]);
        let b = expected.add(Foo::B, &[end]);
        let a = expected.add(Foo::A, &[c]);
        let _root = expected.add(Foo::ROOT, &[c, b, a]);

        println!("expected : {:?}", expected);
        assert_eq!(g, expected);
    }

    #[test]
    pub fn retain_all_but_one() {
        let mut g = Graph::<Foo, Pending>::new();
        let root = g.add(Foo::ROOT, &[]);
        let a = g.add(Foo::A, &[root]);
        let b = g.add(Foo::B, &[root]);
        let c = g.add(Foo::C, &[a, root]);
        let _end = g.add(Foo::END, &[b, c]);

        println!("graph    : {:?}", g);

        g.retain(|_, f| f.requirement != Foo::C);

        println!("retained : {:?}", g);

        let mut expected = Graph::<Foo, Pending>::new();
        let root = expected.add(Foo::ROOT, &[]);
        let a = expected.add(Foo::A, &[root]);
        let b = expected.add(Foo::B, &[root]);
        let _end = expected.add(Foo::END, &[b, a, root]);

        println!("expected : {:?}", expected);
        assert_eq!(g, expected);
    }

    #[test]
    pub fn retain_all_but_two() {
        let mut g = Graph::<Foo, Pending>::new();
        let root = g.add(Foo::ROOT, &[]);
        let a = g.add(Foo::A, &[root]);
        let b = g.add(Foo::B, &[a]);
        let c = g.add(Foo::C, &[a, root]);
        let _end = g.add(Foo::END, &[b, c]);

        println!("graph    : {:?}", g);

        g.retain(|_, f| f.requirement != Foo::A && f.requirement != Foo::B);

        println!("retained : {:?}", g);

        let mut expected = Graph::<Foo, Pending>::new();
        let root = expected.add(Foo::ROOT, &[]);
        let c = expected.add(Foo::C, &[root]);
        let _end = expected.add(Foo::END, &[c, root]);

        println!("expected : {:?}", expected);
        assert_eq!(g, expected);
    }

    #[test]
    pub fn trivial_sequence() {
        let prev = Graph::<Foo, Applied>::new();
        let mut next = Graph::<Foo, Pending>::new();
        let root = next.add(Foo::ROOT, &[]);
        let a = next.add(Foo::A, &[root]);
        let b = next.add(Foo::B, &[root]);
        let c = next.add(Foo::C, &[a, root]);
        let _end = next.add(Foo::END, &[b, c]);

        println!("prev       : {:?}", prev);
        println!("next       : {:?}", next);

        let mut sys = FakeSystem {
            created: Default::default(),
        };

        let cmp = next.compare_with(&mut sys, &prev).unwrap();
        let seq = cmp.generate_application_sequence(&mut sys).unwrap();

        assert_eq!(seq.undo, vec![]);

        assert_eq!(
            seq.todo,
            vec![
                Do {
                    created_by_us: false,
                    should_exist: false,
                    source: GraphNodeReference(0),
                    requirement: &Foo::ROOT,
                },
                Do {
                    created_by_us: false,
                    should_exist: false,
                    source: GraphNodeReference(2),
                    requirement: &Foo::B,
                },
                Do {
                    created_by_us: false,
                    should_exist: false,
                    source: GraphNodeReference(1),
                    requirement: &Foo::A,
                },
                Do {
                    created_by_us: false,
                    should_exist: false,
                    source: GraphNodeReference(3),
                    requirement: &Foo::C,
                },
                Do {
                    created_by_us: false,
                    should_exist: false,
                    source: GraphNodeReference(4),
                    requirement: &Foo::END,
                },
            ]
        );
    }

    #[test]
    pub fn trivial_sequence_backwards() {
        let mut prev = Graph::<Foo, Pending>::new();
        let root = prev.add(Foo::ROOT, &[]);
        let a = prev.add(Foo::A_NOUNDO, &[root]);
        let b = prev.add(Foo::B_NOUNDO, &[root]);
        let c = prev.add(Foo::C, &[a, root]);
        let _end = prev.add(Foo::END, &[b, c]);
        let prev = Graph {
            nodes: prev.nodes,
            state: Applied,
        };

        let next = Graph::<Foo, Pending>::new();

        println!("prev       : {:?}", prev);
        println!("next       : {:?}", next);

        let mut sys = FakeSystem {
            created: Default::default(),
        };

        let cmp = next.compare_with(&mut sys, &prev).unwrap();
        let seq = cmp.generate_application_sequence(&mut sys).unwrap();

        assert_eq!(
            seq.undo,
            vec![
                Undo {
                    pre_existing: false,
                    requirement: &Foo::END,
                },
                Undo {
                    pre_existing: false,
                    requirement: &Foo::C,
                },
                Undo {
                    pre_existing: false,
                    requirement: &Foo::ROOT,
                },
            ]
        );

        assert_eq!(seq.todo, vec![]);
    }

    #[test]
    pub fn inherited_preconditions() {
        let mut prev = Graph::<Foo, Pending>::new();
        let root = prev.add(Foo::ROOT_NOUNDO, &[]);
        let a = prev.add(Foo::A_NOUNDO, &[root]);
        let b = prev.add(Foo::B, &[a, root]);
        let c = prev.add(Foo::C_NOUNDO, &[b]);
        let _end = prev.add(Foo::END, &[b, c]);
        let prev = prev.invert();
        let prev = Graph {
            nodes: prev.nodes,
            state: Applied,
        };

        let next = Graph::<Foo, Pending>::new();

        println!("prev       : {:?}", prev);
        println!("next       : {:?}", next);

        let mut sys = FakeSystem {
            created: Default::default(),
        };

        let cmp = next.compare_with(&mut sys, &prev).unwrap();
        let seq = cmp.generate_application_sequence(&mut sys).unwrap();

        assert_eq!(
            seq.undo,
            vec![
                Undo {
                    pre_existing: false,
                    requirement: &Foo::B,
                },
                Undo {
                    pre_existing: false,
                    requirement: &Foo::END,
                },
            ]
        );

        assert_eq!(seq.todo, vec![]);
    }

    #[test]
    pub fn normal_sequence() {
        let mut prev = Graph::<Foo, Pending>::new();
        let root = prev.add(Foo::ROOT, &[]);
        let a = prev.add(Foo::A, &[root]);
        let _c = prev.add(Foo::C, &[a, root]);
        let _x = prev.add(Foo::D, &[root]);
        let prev = prev.apply_execution_results(ApplyResult {
            pre_existing: Vec::new(),
        });

        let mut next = Graph::<Foo, Pending>::new();
        let root = next.add(Foo::ROOT, &[]);
        let a = next.add(Foo::A, &[root]);
        let b = next.add(Foo::B, &[root]);
        let c = next.add(Foo::C, &[a, root]);
        let _end = next.add(Foo::END, &[b, c]);

        println!("prev       : {:?}", prev);
        println!("next       : {:?}", next);

        let mut sys = FakeSystem {
            created: [PathBuf::from("4")].into_iter().collect(),
        };

        let cmp = next.compare_with(&mut sys, &prev).unwrap();
        let seq = cmp.generate_application_sequence(&mut sys).unwrap();

        assert_eq!(
            seq.undo,
            vec![Undo {
                pre_existing: false,
                requirement: &Foo::D,
            },]
        );

        assert_eq!(
            seq.todo,
            vec![
                Do {
                    created_by_us: true,
                    should_exist: true,
                    source: GraphNodeReference(0),
                    requirement: &Foo::ROOT,
                },
                Do {
                    created_by_us: false,
                    should_exist: false,
                    source: GraphNodeReference(2),
                    requirement: &Foo::B,
                },
                Do {
                    created_by_us: true,
                    should_exist: true,
                    source: GraphNodeReference(1),
                    requirement: &Foo::A,
                },
                Do {
                    created_by_us: true,
                    should_exist: true,
                    source: GraphNodeReference(3),
                    requirement: &Foo::C,
                },
                Do {
                    created_by_us: false,
                    should_exist: false,
                    source: GraphNodeReference(4),
                    requirement: &Foo::END,
                },
            ]
        );
    }

    #[test]
    pub fn apply() {
        let v0 = Graph::<Foo, Applied>::new();
        let mut v1 = Graph::<Foo, Pending>::new();
        let root = v1.add(Foo::ROOT, &[]);
        let a = v1.add(Foo::A, &[root]);
        let b = v1.add(Foo::B, &[root]);
        let c = v1.add(Foo::C, &[a, root]);
        let _end = v1.add(Foo::END, &[b, c]);

        println!("prev       : {:?}", v0);
        println!("next       : {:?}", v1);

        let mut sys = FakeSystem {
            created: Default::default(),
        };

        let cmp = v1.compare_with(&mut sys, &v0).unwrap();
        let seq = cmp.generate_application_sequence(&mut sys).unwrap();
        let results = seq.run(&mut sys, |_| false).unwrap();
        let v1 = v1.apply_execution_results(results);

        assert_eq!(
            sys.created,
            [
                PathBuf::from("0"),
                PathBuf::from("1"),
                PathBuf::from("2"),
                PathBuf::from("3"),
                PathBuf::from("100"),
            ]
            .into_iter()
            .collect()
        );

        let mut v2 = Graph::<Foo, Pending>::new();
        let root = v2.add(Foo::ROOT, &[]);
        let a = v2.add(Foo::A, &[root]);
        let c = v2.add(Foo::C, &[a, root]);
        let _end = v2.add(Foo::END, &[c]);

        println!("prev       : {:?}", v1);
        println!("next       : {:?}", v2);

        let cmp = v2.compare_with(&mut sys, &v1).unwrap();
        let seq = cmp.generate_application_sequence(&mut sys).unwrap();
        let results = seq.run(&mut sys, |_| false).unwrap();
        let _v2 = v2.apply_execution_results(results);

        assert_eq!(
            sys.created,
            [
                PathBuf::from("0"),
                PathBuf::from("1"),
                PathBuf::from("3"),
                PathBuf::from("100"),
            ]
            .into_iter()
            .collect()
        );
    }

    #[test]
    pub fn apply_revert() {
        crate::requirements!(NodeTy = Foo, AlwaysFail);

        let v0 = Graph::<NodeTy, Applied>::new();
        let mut v1 = Graph::<NodeTy, Pending>::new();
        let root = v1.add(Foo::ROOT, &[]);
        let a = v1.add(Foo::A, &[root]);
        let b = v1.add(Foo::B, &[root]);
        let c = v1.add(Foo::C, &[a, root]);
        let _end = v1.add(Foo::END, &[b, c]);

        println!("prev       : {:?}", v0);
        println!("next       : {:?}", v1);

        let mut sys = FakeSystem {
            created: Default::default(),
        };

        let cmp = v1.compare_with(&mut sys, &v0).unwrap();
        let seq = cmp.generate_application_sequence(&mut sys).unwrap();
        let results = seq.run(&mut sys, |_| false).unwrap();
        let v1 = v1.apply_execution_results(results);

        assert_eq!(
            sys.created,
            [
                PathBuf::from("0"),
                PathBuf::from("1"),
                PathBuf::from("2"),
                PathBuf::from("3"),
                PathBuf::from("100"),
            ]
            .into_iter()
            .collect()
        );

        let mut v2 = Graph::<NodeTy, Pending>::new();
        let root = v2.add(Foo::ROOT, &[]);
        let a = v2.add(Foo::A, &[root]);
        let c = v2.add(Foo::C, &[a, root]);
        let end = v2.add(Foo::END, &[c]);
        let _fail = v2.add(AlwaysFail, &[end]);

        println!("prev       : {:?}", v1);
        println!("next       : {:?}", v2);

        let cmp = v2.compare_with(&mut sys, &v1).unwrap();
        let seq = cmp.generate_application_sequence(&mut sys).unwrap();
        let err = seq.run(&mut sys, |_| false).unwrap_err();
        println!("Apply failed successfully");
        println!("System state: {:?}", sys);
        seq.revert(&mut sys, &err.revert_info).unwrap();

        assert_eq!(
            sys.created,
            [
                PathBuf::from("0"),
                PathBuf::from("1"),
                PathBuf::from("2"),
                PathBuf::from("3"),
                PathBuf::from("100"),
            ]
            .into_iter()
            .collect()
        );
    }
}
