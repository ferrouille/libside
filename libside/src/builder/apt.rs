use super::Context;
pub use crate::generic_apt_package;
use crate::graph::GraphNodeReference;
use crate::requirements::{Requirement, Supports};
use crate::system::{NeverError, System};
use serde::{Deserialize, Serialize};
use std::fmt::Display;

#[macro_export]
macro_rules! generic_apt_package {
    ($vis:vis $struct:ident => $apt_package:literal) => {
        $vis struct $struct($crate::graph::GraphNodeReference);

        impl $crate::builder::apt::AptPackage for $struct {
            const NAME: &'static str = $apt_package;

            fn create(node: $crate::graph::GraphNodeReference) -> Self {
                Self(node)
            }

            fn graph_node(&self) -> GraphNodeReference {
                self.0
            }
        }
    }
}

#[derive(Default)]
pub struct Apt {
    update: Option<GraphNodeReference>,
    global_preconditions: Vec<GraphNodeReference>,
}

impl Apt {
    pub fn global_precondition<R: Requirement>(context: &mut Context<R>, node: GraphNodeReference) {
        let state = context.state::<Apt>();
        state.global_preconditions.push(node);
    }
}

pub trait AptPackage {
    const NAME: &'static str;

    fn create(node: GraphNodeReference) -> Self;

    fn graph_node(&self) -> GraphNodeReference;

    fn install<R: Requirement + Supports<AptInstall> + Supports<AptUpdate>>(
        context: &mut Context<R>,
    ) -> Self
    where
        Self: Sized,
    {
        if context.state::<Apt>().update.is_none() {
            context.state::<Apt>().update = Some(context.add_node(AptUpdate, &[]));
        }
        let state = context.state::<Apt>();
        let updated = state.update;
        let dependencies = updated
            .iter()
            .chain(state.global_preconditions.iter())
            .copied()
            .collect::<Vec<_>>();
        Self::create(context.add_node(AptInstall::new(Self::NAME), dependencies.iter()))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
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

    #[error("apt-get failed: {0} {1}")]
    Unsuccessful(String, String),
}

impl<S: System> From<(&str, &str)> for InstallError<S> {
    fn from(output: (&str, &str)) -> Self {
        InstallError::Unsuccessful(output.0.to_string(), output.1.to_string())
    }
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
            )
            .map_err(InstallError::FailedToStart)?;
        result.successful()?;

        Ok(())
    }

    fn modify<S: crate::system::System>(
        &self,
        _system: &mut S,
    ) -> Result<(), Self::ModifyError<S>> {
        Ok(())
    }

    fn delete<S: crate::system::System>(&self, system: &mut S) -> Result<(), Self::DeleteError<S>> {
        let result = system
            .execute_command("apt-get", &["remove", "-y", "-q", &self.name])
            .map_err(InstallError::FailedToStart)?;
        result.successful()?;

        Ok(())
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AptUpdate;

#[derive(Debug, thiserror::Error)]
#[error("unable to execute apt-get: {0}")]
pub struct UpdateError<S: System>(S::CommandError);

impl Requirement for AptUpdate {
    const NAME: &'static str = "apt_update";

    type CreateError<S: System> = UpdateError<S>;
    type ModifyError<S: System> = UpdateError<S>;
    type DeleteError<S: System> = NeverError;
    type HasBeenCreatedError<S: System> = NeverError;

    fn create<S: System>(&self, system: &mut S) -> Result<(), Self::CreateError<S>> {
        system
            .execute_command("apt-get", &["update"])
            .map_err(UpdateError)
            .map(|_| ())
    }

    fn modify<S: System>(&self, system: &mut S) -> Result<(), Self::ModifyError<S>> {
        self.create(system)
    }

    fn delete<S: System>(&self, _system: &mut S) -> Result<(), Self::DeleteError<S>> {
        Ok(())
    }

    fn has_been_created<S: System>(
        &self,
        _system: &mut S,
    ) -> Result<bool, Self::HasBeenCreatedError<S>> {
        Ok(true)
    }

    fn affects(&self, _other: &Self) -> bool {
        true
    }

    fn supports_modifications(&self) -> bool {
        true
    }

    fn can_undo(&self) -> bool {
        false
    }

    fn may_pre_exist(&self) -> bool {
        true
    }

    fn verify<S: System>(&self, _system: &mut S) -> Result<bool, ()> {
        Ok(true)
    }
}

impl Display for AptUpdate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "apt-update")
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        builder::apt::{AptInstall, AptUpdate},
        requirements::Requirement,
        testing::LxcInstance,
    };

    #[test]
    pub fn serialize_deserialize_apt_install() {
        let r = AptInstall {
            name: "test".to_string(),
        };
        let json = r#"{"name":"test"}"#;

        assert_eq!(serde_json::to_string(&r).unwrap(), json);
        assert_eq!(r, serde_json::from_str(json).unwrap());
    }

    #[test]
    pub fn serialize_deserialize_apt_update() {
        let r = AptUpdate;
        let json = r#"null"#;

        assert_eq!(serde_json::to_string(&r).unwrap(), json);
        assert_eq!(r, serde_json::from_str(json).unwrap());
    }

    #[test]
    #[ignore]
    pub fn lxc_apt_install() {
        let mut sys = LxcInstance::start(LxcInstance::DEFAULT_IMAGE);
        let p = AptInstall {
            name: "nginx".to_string(),
        };

        assert!(!p.has_been_created(&mut sys).unwrap());
        assert!(!p.verify(&mut sys).unwrap());

        p.create(&mut sys).unwrap();

        assert!(p.has_been_created(&mut sys).unwrap());
        assert!(p.verify(&mut sys).unwrap());

        p.delete(&mut sys).unwrap();

        assert!(!p.has_been_created(&mut sys).unwrap());
        assert!(!p.verify(&mut sys).unwrap());
    }

    #[test]
    #[ignore]
    pub fn lxc_apt_update() {
        let mut sys = LxcInstance::start(LxcInstance::DEFAULT_IMAGE);
        let p = AptUpdate;

        p.create(&mut sys).unwrap();
        p.modify(&mut sys).unwrap();
        p.delete(&mut sys).unwrap();
    }
}
