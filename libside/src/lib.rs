#![feature(generic_associated_types, negative_impls, auto_traits)]

use crate::{builder::Packages, graph::VerificationState};
use apply::SystemState;
use builder::{fs::CreateDirectory, Builder};
use requirements::{Requirement, Supports};
use serde::{de::DeserializeOwned, Serialize};
use std::{
    io::{BufRead, Cursor},
    num::ParseIntError,
    path::{Path, PathBuf},
};
use structopt::StructOpt;
use system::System;

pub use libside_procmacro::config_file;

pub mod apply;
pub mod builder;
pub mod config;
pub mod graph;
pub mod requirements;
pub mod secrets;
pub mod system;
pub mod testing;
pub mod utils;

#[derive(Debug, thiserror::Error)]
pub enum RunError<S: System, B: Builder> {
    #[error("The applications executable could not be located: {}", .0)]
    CurrentExeNotFound(std::io::Error),

    #[error("Initialization failed: {}", .0)]
    InitFailed(InitError<S>),

    #[error("Build failed: {}", .0)]
    BuildFailed(BuildError<S, B>),

    #[error("Verification failed")]
    VerificationFailed,
}

#[derive(Debug, thiserror::Error)]
pub enum InitError<S: System> {
    #[error("The base directory {:?} is not empty", .0)]
    BaseDirectoryNotEmpty(PathBuf),

    #[error("Could not open {:?}: {}", .0, .1)]
    UnableToOpen(PathBuf, S::Error),

    #[error("Unable to create directory {:?}: {}", .0, .1)]
    UnableToCreateDir(PathBuf, S::Error),

    #[error("Unable to write current version to {:?}: {}", .0, .1)]
    UnableToWriteCurrentVersion(PathBuf, S::Error),

    #[error("Unable to write database: {}", .0)]
    UnableToWriteDb(DbWriteError<S>),
}

#[derive(Debug, thiserror::Error)]
pub enum DbWriteError<S: System> {
    #[error("Unable to write database to {}: {}", .0.display(), .1)]
    UnableToWriteCurrentVersion(PathBuf, S::Error),

    #[error("Unable to serialize database: {}", .0)]
    UnableToSerialize(serde_json::Error),

    #[error("Unable to create database {}: {}", .0.display(), .1)]
    UnableToCreateDb(PathBuf, S::Error),
}

#[derive(Debug, thiserror::Error)]
pub enum GetCurrentStateError<S: System> {
    #[error("Unable to read {:?}: {}", .0, .1)]
    UnableToReadCurrentState(PathBuf, S::Error),

    #[error("Failed to parse the current version number: {}", .0)]
    CurrentNotANumber(ParseIntError),
}

#[derive(Debug, thiserror::Error)]
pub enum BuildError<S: System, B: Builder> {
    #[error("Build failed: {}", .0)]
    BuildFailed(B::BuildError),

    #[error("Unable to generate files needed for the build: ")]
    UnableToGenerateFiles(()),

    #[error("Unable to change the current install: {}", .0)]
    UnableToChangeCurrentInstall(S::Error),

    #[error("Unable to determine differences with previous build: {}", .0)]
    DiffFailed(<B::Requirement as Requirement>::HasBeenCreatedError<S>),

    #[error("Unable to generate an application sequence: ")]
    ApplicationSequenceGenerationFailed(()),

    #[error("Unable to apply the build: {}", .0)]
    ApplyFailed(graph::RunError<B::Requirement, S>),

    #[error("Unable to save new state: ")]
    SaveError(()),
}

impl<S: System, B: Builder> From<BuildError<S, B>> for RunError<S, B> {
    fn from(err: BuildError<S, B>) -> Self {
        RunError::BuildFailed(err)
    }
}

pub struct Dirs {
    base: PathBuf,

    /// /srv/packages
    packages: PathBuf,

    /// /srv/installed
    installed: PathBuf,

    /// /srv/chroots/
    chroots: PathBuf,

    /// /srv/files/exposed
    files_exposed: PathBuf,

    /// /srv/files/config
    files_config: PathBuf,

    /// /srv/files/deleted
    deleted: PathBuf,

    /// /srv/data
    data: PathBuf,

    /// /srv/backups
    backups: PathBuf,

    /// /srv/secrets
    secrets: PathBuf,
}

fn create_dir_with_err<S: System>(system: &mut S, dir: &Path) -> Result<(), InitError<S>> {
    system
        .make_dir_all(dir)
        .map_err(|e| InitError::UnableToCreateDir(dir.to_owned(), e))
}

impl Dirs {
    pub fn new<P: AsRef<Path>>(base: P) -> Self {
        let base = base.as_ref();
        Dirs {
            base: base.to_owned(),
            packages: base.join("packages"),
            installed: base.join("installed"),
            files_exposed: base.join("files/exposed"),
            files_config: base.join("files/config"),
            deleted: base.join("files/deleted"),
            chroots: base.join("chroots"),
            data: base.join("data"),
            backups: base.join("backups"),
            secrets: base.join("secrets"),
        }
    }

    pub fn get_install(&self, version: u64) -> StateDirs {
        let v = version.to_string();
        let versioned_base = self.installed.join(&v);

        StateDirs {
            version,
            db: versioned_base.join("db"),
            generated: versioned_base.join("generated"),
            config: self.files_config.clone(),
            base: versioned_base,
            chroots: self.chroots.join(&v),
            files_exposed: self.files_exposed.clone(),
            data: self.data.clone(),
            backup: self.backups.clone(),
        }
    }

    fn current_path(&self) -> PathBuf {
        self.installed.join("current")
    }

    pub fn current_install<S: System>(
        &self,
        system: &mut S,
    ) -> Result<StateDirs, GetCurrentStateError<S>> {
        let current = self.current_path();
        let current = String::from_utf8(
            system
                .file_contents(&current)
                .map_err(|e| GetCurrentStateError::UnableToReadCurrentState(current, e))?,
        )
        .unwrap();
        let version = current
            .parse::<u64>()
            .map_err(GetCurrentStateError::CurrentNotANumber)?;

        Ok(self.get_install(version))
    }

    pub fn set_current_install<S: System>(
        &self,
        new: &StateDirs,
        system: &mut S,
    ) -> Result<(), S::Error> {
        let current = self.current_path();
        system.put_file_contents(&current, format!("{}", new.version).as_bytes())
    }

    pub fn fresh_install<S: System>(
        &self,
        system: &mut S,
    ) -> Result<StateDirs, GetCurrentStateError<S>> {
        let mut max = 0u64;
        for dir in system.read_dir(&self.installed).unwrap() {
            match dir.parse::<u64>() {
                Ok(n) => max = max.max(n),
                _ => (),
            }
        }

        let next_version = max + 1;
        Ok(self.get_install(next_version))
    }

    pub fn initialize<R: Requirement, S: System>(
        &self,
        system: &mut S,
    ) -> Result<(), InitError<S>> {
        create_dir_with_err(system, &self.base)?;

        // Make sure nobody besides us can read the base dir
        system.chmod(&self.base, 0o700).unwrap();

        if !system
            .dir_is_empty(&self.base)
            .map_err(|e| InitError::UnableToOpen(self.base.to_owned(), e))?
        {
            return Err(InitError::BaseDirectoryNotEmpty(self.base.to_owned()));
        }

        create_dir_with_err(system, &self.packages)?;
        create_dir_with_err(system, &self.installed)?;
        create_dir_with_err(system, &self.chroots)?;
        create_dir_with_err(system, &self.files_exposed)?;
        create_dir_with_err(system, &self.files_config)?;
        create_dir_with_err(system, &self.data)?;
        create_dir_with_err(system, &self.backups)?;
        create_dir_with_err(system, &self.secrets)?;

        let install = self.get_install(0);
        install.create_dirs(system)?;
        install
            .write_dbs(system, &SystemState::<R>::default())
            .map_err(InitError::UnableToWriteDb)?;

        self.set_current_install(&install, system)
            .map_err(|e| InitError::UnableToWriteCurrentVersion(self.current_path(), e))?;

        Ok(())
    }
}

pub struct VersionedPath {
    path: PathBuf,
    version: u64,
}

impl VersionedPath {
    pub fn join<P: AsRef<Path>>(&self, other: P) -> VersionedPath {
        VersionedPath {
            path: self.path.join(other),
            version: self.version,
        }
    }

    pub fn unversioned_path(&self) -> &Path {
        self.path.as_path()
    }

    pub fn full_path(&self) -> PathBuf {
        self.path.join(self.version.to_string())
    }
}

pub struct StateDirs {
    version: u64,
    base: PathBuf,
    db: PathBuf,
    generated: PathBuf,
    chroots: PathBuf,
    data: PathBuf,
    config: PathBuf,
    files_exposed: PathBuf,
    backup: PathBuf,
}

impl StateDirs {
    pub fn db(&self) -> &Path {
        &self.db
    }

    pub fn create_dirs<S: System>(&self, system: &mut S) -> Result<(), InitError<S>> {
        create_dir_with_err(system, &self.generated)?;
        create_dir_with_err(system, &self.chroots)?;

        Ok(())
    }

    pub fn exposed_path(&self, name: &str) -> VersionedPath {
        VersionedPath {
            path: self.files_exposed.join(name),
            version: self.version,
        }
    }

    pub fn generated_path(&self, name: &str) -> PathBuf {
        self.generated.join(name)
    }

    pub fn chroot_path(&self, name: &str) -> PathBuf {
        self.chroots.join(name)
    }

    pub fn config_path(&self, name: &str) -> PathBuf {
        self.config.join(name)
    }

    pub fn userdata_path(&self, name: &str) -> PathBuf {
        self.data.join(name).join("userdata")
    }

    pub fn backup_path(&self, name: &str) -> PathBuf {
        self.backup.join(name)
    }

    pub fn deleted_file_backup_path(&self, name: &str) -> PathBuf {
        self.generated.join(name).join("deleted-file-backup")
    }

    pub fn load_install<R: DeserializeOwned, S: System>(&self, system: &mut S) -> SystemState<R> {
        let contents = system.file_contents(&self.db).unwrap();

        SystemState {
            graph: serde_json::from_reader(&mut Cursor::new(contents)).unwrap(),
        }
    }

    pub fn write_dbs<R: Serialize, S: System>(
        &self,
        system: &mut S,
        dbs: &SystemState<R>,
    ) -> Result<(), DbWriteError<S>> {
        let contents =
            serde_json::to_string(&dbs.graph).map_err(DbWriteError::UnableToSerialize)?;
        system
            .put_file_contents(&self.db, contents.as_bytes())
            .map_err(|e| DbWriteError::UnableToCreateDb(self.db.clone(), e))?;

        Ok(())
    }
}

pub struct SiDe {}

#[derive(StructOpt)]
pub enum Command {
    Init,
    Status,
    Build {
        #[structopt(long = "ignore-verification")]
        ignore_verification: bool,

        #[structopt(long = "ask-overwrite")]
        ask_overwrite: bool,
    },
    Apply {
        target: u64,

        #[structopt(long = "ignore-verification")]
        ignore_verification: bool,

        #[structopt(long = "ask-overwrite")]
        ask_overwrite: bool,
    },
    Verify {
        #[structopt(long = "fix")]
        fix: bool,
    },
}

#[derive(StructOpt)]
pub struct Args {
    base_dir: PathBuf,

    #[structopt(subcommand)]
    command: Command,
}

#[derive(Serialize)]
pub struct Status {
    current_version: u64,
    base_path: PathBuf,
    backup_path: PathBuf,
}

impl SiDe {
    pub fn run<S: System, B: Builder>(system: &mut S, builder: B) -> Result<(), RunError<S, B>>
    where
        B::Requirement: Supports<CreateDirectory>,
    {
        let args = Args::from_args();
        let dirs = Dirs::new(&args.base_dir);

        Self::run_command(args.command, &dirs, system, builder)
    }

    pub fn run_command<S: System, B: Builder>(
        command: Command,
        dirs: &Dirs,
        system: &mut S,
        builder: B,
    ) -> Result<(), RunError<S, B>>
    where
        B::Requirement: Supports<CreateDirectory>,
    {
        match command {
            Command::Init => {
                dirs.initialize::<B::Requirement, S>(system)
                    .map_err(RunError::InitFailed)?;

                Ok(())
            }
            Command::Status => {
                let current = dirs.current_install(system).unwrap();

                println!(
                    "{}",
                    serde_json::to_string_pretty(&Status {
                        current_version: current.version,
                        base_path: dirs.base.clone(),
                        backup_path: dirs.backups.clone(),
                    })
                    .unwrap()
                );

                Ok(())
            }
            Command::Apply {
                target,
                ignore_verification,
                ask_overwrite,
            } => {
                let current = dirs.current_install(system).unwrap();
                let target = dirs.get_install(target);
                let current_state = current.load_install::<B::Requirement, S>(system);
                let target_state = target.load_install::<B::Requirement, S>(system);

                if ignore_verification {
                    println!("Skipping verification of current state...");
                } else {
                    println!("Verifying current state...");
                    match current_state.verify_system_state(system).unwrap() {
                        VerificationState::Ok => println!("Verification OK"),
                        err @ VerificationState::Invalid { .. } => {
                            panic!("Verification failed:\n{}", err)
                        }
                    }
                }

                println!("Current: {}", current.version);
                println!("Target : {}", target.version);

                let cmp = target_state
                    .graph
                    .compare_with(system, &current_state.graph)
                    .map_err(BuildError::DiffFailed)?;
                let instructions = cmp
                    .generate_application_sequence(system)
                    .map_err(BuildError::ApplicationSequenceGenerationFailed)?;

                // The result returned by run describes which requirements were pre-existing;
                // That's not relevant to us, because we want to keep the original values that we determined when we created this install.
                match instructions.run(system, |s| {
                    if ask_overwrite {
                        println!("Can {} be overwritten? Type 'yes' to continue or anything else to abort", s);
                        let line = std::io::stdin().lock().lines().next().unwrap().unwrap();
                        return line.trim() == "yes";
                    }

                    return false;
                }) {
                    Ok(_) => {}
                    Err(err) => {
                        println!();
                        println!("Error: {}", err);
                        println!("Reverting...");
                        instructions
                            .revert(system, &err.revert_info)
                            .unwrap();

                        println!("Revert OK");
                        return Err(BuildError::ApplyFailed(err).into());
                    }
                }

                dirs.set_current_install(&target, system)
                    .map_err(BuildError::UnableToChangeCurrentInstall)?;
                println!("Done!");

                Ok(())
            }
            Command::Build {
                ignore_verification,
                ask_overwrite,
            } => {
                let current = dirs.current_install(system).unwrap();
                let current_state = current.load_install::<B::Requirement, S>(system);

                if ignore_verification {
                    println!("Skipping verification of current state...");
                } else {
                    println!("Verifying current state...");
                    match current_state.verify_system_state(system).unwrap() {
                        VerificationState::Ok => println!("Verification OK"),
                        err @ VerificationState::Invalid { .. } => {
                            panic!("Verification failed:\n{}", err)
                        }
                    }
                }

                let new_install = dirs.fresh_install(system).unwrap();
                println!("Current install: {}", current.base.display());
                println!("New install: {}", new_install.base.display());

                let packages = Packages::load(&dirs, system).unwrap();
                let prepared = builder::run(&dirs, system, packages, &new_install, builder)
                    .map_err(BuildError::BuildFailed)?;

                let graph = prepared
                    .generate_files(system, &current_state)
                    .map_err(BuildError::UnableToGenerateFiles)?;
                let cmp = graph
                    .compare_with(system, &current_state.graph)
                    .map_err(BuildError::DiffFailed)?;
                let instructions = cmp
                    .generate_application_sequence(system)
                    .map_err(BuildError::ApplicationSequenceGenerationFailed)?;
                match instructions.run(system, |s| {
                    if ask_overwrite {
                        println!("Can {} be overwritten? Type 'yes' to continue or anything else to abort", s);
                        let line = std::io::stdin().lock().lines().next().unwrap().unwrap();
                        return line.trim() == "yes";
                    }

                    return false;
                }) {
                    Ok(result) => {
                        let _new_state = prepared.save(system, result).map_err(BuildError::SaveError)?;
                        dirs.set_current_install(&new_install, system)
                            .map_err(BuildError::UnableToChangeCurrentInstall)?;

                        Ok(())
                    }
                    Err(err) => {
                        println!();
                        println!("Error: {}", err);
                        println!("Reverting...");
                        instructions
                            .revert(system, &err.revert_info)
                            .unwrap();

                        println!("Revert OK");
                        Err(BuildError::ApplyFailed(err).into())
                    }
                }
            }
            Command::Verify { fix } => {
                let current = dirs.current_install(system).unwrap();
                println!("Current install: {}", current.base.display());

                let current_state = current.load_install::<B::Requirement, S>(system);
                match current_state.verify_system_state(system).unwrap() {
                    VerificationState::Ok => println!("Verification OK"),
                    err @ VerificationState::Invalid { .. } => {
                        println!("Verification failed:\n{}", err);

                        if fix {
                            let seq = current_state.graph.generate_fix_sequence(system).unwrap();

                            // The result returned by run describes which requirements were pre-existing;
                            // That's not relevant to us, because we want to keep the original values that we determined when we created this install.
                            let _ = seq.run(system, |_| false).unwrap();

                            println!("Fixing successful!");
                        } else {
                            return Err(RunError::VerificationFailed);
                        }
                    }
                }

                Ok(())
            }
        }
    }
}
