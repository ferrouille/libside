use super::Context;
use crate::graph::GraphNodeReference;
use crate::requirements::{Requirement, Supports};
use crate::system::{NeverError, System};
use serde::{Deserialize, Serialize};
use std::fmt::Display;

pub struct AptPackage<const NAME: &'static str>(GraphNodeReference);

impl<const NAME: &'static str> AptPackage<NAME> {
    pub fn install<R: Requirement + Supports<AptInstall>>(
        context: &mut Context<R>,
    ) -> AptPackage<NAME> {
        // TODO: Prevent installing twice
        AptPackage(context.add_node(AptInstall::new(NAME), &[]))
    }

    pub fn graph_node(&self) -> GraphNodeReference {
        self.0
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AptInstall {
    name: String,
}

impl AptInstall {
    pub fn new(name: &str) -> AptInstall {
        AptInstall {
            name: name.to_string(),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum InstallError<S: System> {
    #[error("unable to execute apt-get: {0}")]
    FailedToStart(S::CommandError),

    #[error("apt-get failed: {0}")]
    Unsuccessful(String),
}

#[derive(Debug, thiserror::Error)]
#[error("unable to execute apt-get: {0}")]
pub struct CheckError<S: System>(S::CommandError);

impl Requirement for AptInstall {
    type CreateError<S: System> = InstallError<S>;
    type ModifyError<S: System> = NeverError;
    type DeleteError<S: System> = InstallError<S>;
    type HasBeenCreatedError<S: System> = CheckError<S>;

    fn create<S: crate::system::System>(&self, system: &mut S) -> Result<(), Self::CreateError<S>> {
        println!("  install: {}", self.name);
        let result = system
            .execute_command(
                "apt-get",
                &[
                    "install",
                    "-y",
                    "-q",
                    "--no-install-recommends",
                    self.name.as_str(),
                ],
            ).map_err(InstallError::FailedToStart)?;
        
        result.successful().map_err(|(stdout, stderr)| InstallError::Unsuccessful(format!("{stdout}\n{stderr}")))
    }

    fn modify<S: crate::system::System>(
        &self,
        _system: &mut S,
    ) -> Result<(), Self::ModifyError<S>> {
        Ok(())
    }

    fn delete<S: crate::system::System>(&self, system: &mut S) -> Result<(), Self::DeleteError<S>> {
        println!("  uninstall: {}", self.name);
        let result = system
            .execute_command("apt-get", &["remove", "-y", "-q", &self.name])
            .map_err(InstallError::FailedToStart)?;
        
        result.successful().map_err(|(stdout, stderr)| InstallError::Unsuccessful(format!("{stdout}\n{stderr}")))
    }

    fn has_been_created<S: crate::system::System>(
        &self,
        system: &mut S,
    ) -> Result<bool, Self::HasBeenCreatedError<S>> {
        let result = system
            .execute_command("dpkg-query", &["-W", "-f=${Status}", &self.name])
            .map_err(CheckError)?;
        if result.is_success() {
            Ok(result.stdout_as_str().starts_with("install"))
        } else {
            Ok(false)
        }
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
        true
    }

    fn verify<S: System>(&self, system: &mut S) -> Result<bool, ()> {
        Ok(self.has_been_created(system).unwrap())
    }

    const NAME: &'static str = "apt_package";
}

impl Display for AptInstall {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "apt({})", self.name)
    }
}
