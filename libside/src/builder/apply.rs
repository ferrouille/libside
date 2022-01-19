use super::MinimalContext;
use crate::apply::SystemState;
use crate::requirements::Requirement;
use crate::system::System;
use crate::{
    graph::{ApplyResult, Graph, Pending},
    StateDirs,
};
use std::{
    fs,
    path::{Path, PathBuf},
};

pub struct PreparedBuild<'d, R> {
    contexts: Vec<MinimalContext>,
    install: &'d StateDirs,
    target_graph: Graph<R, Pending>,
}

impl<'d, R: Requirement> PreparedBuild<'d, R> {
    pub fn new(
        install: &'d StateDirs,
        contexts: Vec<MinimalContext>,
        graph: Graph<R, Pending>,
    ) -> Self {
        PreparedBuild {
            contexts,
            install,
            target_graph: graph,
        }
    }

    pub fn generate_files<'r, S: System>(
        &self,
        _system: &mut S,
        _prev: &SystemState<R>,
    ) -> Result<&Graph<R, Pending>, ()> {
        // TODO: Use system to create the files
        self.install.create_dirs().unwrap();

        // Generate the config files, because we need them for the install
        for config in self.contexts.iter().map(|c| c.files.iter()).flatten() {
            let path = config.source.parent().unwrap();
            println!("  prep : {}", config.source.display());
            fs::create_dir_all(&path).unwrap();
            fs::write(&config.source, &config.contents).unwrap();
        }

        for deleted in self
            .contexts
            .iter()
            .map(|c| c.deleted_files.iter())
            .flatten()
        {
            let path = deleted.save_to.parent().unwrap();
            println!("  prep : {}", path.display());
            fs::create_dir_all(&path).unwrap();
        }

        // Create the main application files
        // TODO: Should we allow custom owners for exposed files, or should we keep everything owned by root? Does it even matter if we don't need the files to ever be writeable?
        for context in self.contexts.iter() {
            for exposed in context.exposed.iter() {
                println!("  expose: {:?}", exposed.source);
                let metadata = exposed.source.symlink_metadata().unwrap();
                if metadata.file_type().is_dir() {
                    fs::create_dir_all(&exposed.target).unwrap();
                    copy(&exposed.source, &exposed.target).unwrap();
                } else {
                    copy_file(&exposed.source, &exposed.target).unwrap();
                }
            }
        }

        Ok(&self.target_graph)
    }

    pub fn save(self, result: ApplyResult) -> Result<SystemState<R>, ()> {
        let state = SystemState {
            graph: self.target_graph.apply_execution_results(result),
        };

        self.install.write_dbs(&state).unwrap();
        Ok(state)
    }
}

pub fn copy<U: AsRef<Path>, V: AsRef<Path>>(from: U, to: V) -> Result<(), std::io::Error> {
    let mut stack = Vec::new();
    stack.push(PathBuf::from(from.as_ref()));

    let output_root = PathBuf::from(to.as_ref());
    let input_root = PathBuf::from(from.as_ref()).components().count();

    while let Some(working_path) = stack.pop() {
        // Generate a relative path
        let src: PathBuf = working_path.components().skip(input_root).collect();

        // Create a destination if missing
        let dest = if src.components().count() == 0 {
            output_root.clone()
        } else {
            output_root.join(&src)
        };
        if fs::metadata(&dest).is_err() {
            fs::create_dir_all(&dest)?;
        }

        for entry in fs::read_dir(working_path)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else {
                match path.file_name() {
                    Some(filename) => {
                        let dest_path = dest.join(filename);
                        copy_file(&path, &dest_path)?;
                    }
                    None => {
                        panic!("failed to copy: {:?}", path);
                    }
                }
            }
        }
    }

    Ok(())
}

fn copy_file(path: &PathBuf, dest: &PathBuf) -> Result<(), std::io::Error> {
    Ok(if path.is_symlink() {
        todo!()
    } else {
        fs::copy(&path, &dest)?;
        // TODO: chown, chgroup, chmod
    })
}
