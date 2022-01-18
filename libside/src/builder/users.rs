use super::{AsParam, Context};
use crate::system::System;
use crate::{
    graph::GraphNodeReference,
    requirements::{Requirement, Supports},
    system::NeverError,
};
use itertools::Itertools;
use serde::{Deserialize, Serialize};
use std::{
    fmt::Display,
    io::{BufRead, BufReader, Cursor},
    path::PathBuf,
};

pub struct User {
    pub(crate) name: String,
    pub(crate) node: GraphNodeReference,
}

impl User {
    pub fn add<'r, R: Requirement + Supports<CreateUser>>(
        context: &mut Context<R>,
        name: &str,
        create: impl for<'c> FnOnce(&'c mut UserConfig<'r>) -> &'c mut UserConfig<'r>,
    ) -> (User, Group) {
        let mut info = UserConfig {
            system: true,
            supplementary_groups: Vec::new(),
        };

        create(&mut info);

        let node = context.add_node(
            CreateUser {
                name: name.to_string(),
                system: info.system,
                supplementary_groups: info
                    .supplementary_groups
                    .iter()
                    .map(|g| g.name.clone())
                    .collect(),
                shell: "/bin/false".to_owned(),
                home_dir: None,
            },
            info.supplementary_groups.iter().map(|g| &g.node),
        );

        (
            User {
                name: name.to_string(),
                node,
            },
            Group {
                name: name.to_string(),
                node,
            },
        )
    }

    pub fn graph_node(&self) -> GraphNodeReference {
        self.node
    }
}

impl AsParam for User {
    fn as_param(&self) -> String {
        self.name.clone()
    }
}

pub struct Group {
    pub(crate) name: String,
    pub(crate) node: GraphNodeReference,
}

impl Group {
    pub fn add<R: Requirement + Supports<CreateGroup>>(
        context: &mut Context<R>,
        name: &str,
        system: bool,
    ) -> Group {
        Group {
            name: name.to_owned(),
            node: context.add_node(CreateGroup::new(name, system), &[]),
        }
    }

    pub fn graph_node(&self) -> GraphNodeReference {
        self.node
    }
}

impl AsParam for Group {
    fn as_param(&self) -> String {
        self.name.clone()
    }
}

pub struct UserConfig<'r> {
    system: bool,
    supplementary_groups: Vec<&'r Group>,
}

impl<'r> UserConfig<'r> {
    pub fn system(&mut self, val: bool) -> &mut Self {
        self.system = val;
        self
    }

    pub fn add_group(&mut self, group: &'r Group) -> &mut Self {
        self.supplementary_groups.push(group);
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CreateUser {
    pub(crate) name: String,
    pub(crate) system: bool,
    pub(crate) supplementary_groups: Vec<String>,
    pub(crate) shell: String,
    pub(crate) home_dir: Option<String>,
}

impl Requirement for CreateUser {
    type CreateError<S: System> = NeverError;
    type ModifyError<S: System> = NeverError;
    type DeleteError<S: System> = NeverError;
    type HasBeenCreatedError<S: System> = NeverError;

    fn create<S: crate::system::System>(&self, system: &mut S) -> Result<(), Self::CreateError<S>> {
        println!("  useradd: {}", self.name);
        let group_str;
        let mut args = Vec::new();

        if self.system {
            args.push("--system");
        }

        if let Some(home_dir) = &self.home_dir {
            args.push("--create-home");
            args.push("--home-dir");
            args.push(home_dir.as_str());
        } else {
            args.push("--no-create-home");
        }

        if self.supplementary_groups.len() > 0 {
            group_str = self.supplementary_groups.iter().join(",");

            args.push("--groups");
            args.push(&group_str);
        }

        args.push("--shell");
        args.push(&self.shell);

        args.push(&self.name);

        let result = system.execute_command("useradd", &args).unwrap();
        assert!(result.is_success()); // TODO

        Ok(())
    }

    fn modify<S: crate::system::System>(&self, system: &mut S) -> Result<(), Self::ModifyError<S>> {
        println!("  usermod: {}", self.name);
        let group_str;
        let mut args = Vec::new();

        if let Some(home_dir) = &self.home_dir {
            args.push("--home");
            args.push(&home_dir);
        }

        if self.supplementary_groups.len() > 0 {
            group_str = self.supplementary_groups.iter().join(",");

            args.push("--groups");
            args.push(&group_str);
        }

        args.push("--shell");
        args.push(&self.shell);

        args.push(&self.name);

        let result = system.execute_command("usermod", &args).unwrap();
        assert!(result.is_success()); // TODO

        Ok(())
    }

    fn delete<S: crate::system::System>(&self, system: &mut S) -> Result<(), Self::DeleteError<S>> {
        println!("  userdel: {}", self.name);
        let result = system.execute_command("userdel", &[&self.name]).unwrap();
        assert!(result.is_success()); // TODO

        Ok(())
    }

    fn has_been_created<S: crate::system::System>(
        &self,
        system: &mut S,
    ) -> Result<bool, Self::HasBeenCreatedError<S>> {
        Ok(system.get_user(&self.name).unwrap().is_some())
    }

    fn affects(&self, other: &Self) -> bool {
        self.name == other.name
    }

    fn supports_modifications(&self) -> bool {
        true
    }
    fn can_undo(&self) -> bool {
        true
    }
    fn may_pre_exist(&self) -> bool {
        false
    }

    fn verify<S: System>(&self, system: &mut S) -> Result<bool, ()> {
        let user = system.get_user(&self.name).unwrap();

        // TODO: Verify other properties of this user

        Ok(user.is_some())
    }

    const NAME: &'static str = "user";
}

impl Display for CreateUser {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "user({})", self.name)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateGroup {
    name: String,
    system: bool,
}

impl CreateGroup {
    pub fn new(name: &str, system: bool) -> Self {
        Self {
            name: name.to_string(),
            system,
        }
    }
}

impl Requirement for CreateGroup {
    type CreateError<S: System> = NeverError;
    type ModifyError<S: System> = NeverError;
    type DeleteError<S: System> = NeverError;
    type HasBeenCreatedError<S: System> = NeverError;

    fn create<S: crate::system::System>(&self, system: &mut S) -> Result<(), Self::CreateError<S>> {
        println!("  groupadd: {}", self.name);
        let mut args = Vec::new();
        if self.system {
            args.push("--system");
        }

        args.push(&self.name);

        let result = system.execute_command("groupadd", &args).unwrap();
        assert!(result.is_success()); // TODO

        Ok(())
    }

    fn modify<S: crate::system::System>(
        &self,
        _system: &mut S,
    ) -> Result<(), Self::ModifyError<S>> {
        Ok(())
    }

    fn delete<S: crate::system::System>(&self, system: &mut S) -> Result<(), Self::DeleteError<S>> {
        println!("  groupdel: {}", self.name);
        let result = system.execute_command("groupdel", &[&self.name]).unwrap();
        assert!(result.is_success()); // TODO

        Ok(())
    }

    fn has_been_created<S: crate::system::System>(
        &self,
        system: &mut S,
    ) -> Result<bool, Self::HasBeenCreatedError<S>> {
        let file = system.file_contents(&PathBuf::from("/etc/group")).unwrap();
        let reader = BufReader::new(Cursor::new(file));
        let search_str = format!("{}:", self.name);
        for line in reader.lines() {
            if line.unwrap().starts_with(&search_str) {
                return Ok(true);
            }
        }

        return Ok(false);
    }

    fn affects(&self, other: &Self) -> bool {
        self.name == other.name
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

    fn verify<S: System>(&self, system: &mut S) -> Result<bool, ()> {
        Ok(self.has_been_created(system).unwrap())
    }

    const NAME: &'static str = "group";
}

impl Display for CreateGroup {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "group({})", self.name)
    }
}
