use libside::{
    builder::{fs::CreateDirectory, Builder},
    requirements,
    testing::LxcInstance,
    Command, Dirs, SiDe,
};

#[derive(Copy, Clone, Debug, thiserror::Error)]
#[error("Empty error")]
struct EmptyError;

#[derive(Clone, Debug)]
struct EmptyBuilder;

impl Builder for EmptyBuilder {
    type PackageConfig = ();
    type Data = ();
    type Requirement = requirements!(CreateDirectory);
    type BuildError = EmptyError;

    fn start_build(
        &self,
        _context: &mut libside::builder::Context<Self::Requirement>,
    ) -> Result<Self::Data, Self::BuildError> {
        Ok(())
    }

    fn build_package(
        &self,
        _package: &libside::builder::Package<Self::PackageConfig>,
        _context: &mut libside::builder::Context<Self::Requirement>,
        _data: &mut Self::Data,
    ) -> Result<(), Self::BuildError> {
        Ok(())
    }

    fn finish_build(
        &self,
        _context: &mut libside::builder::Context<Self::Requirement>,
        _data: Self::Data,
    ) -> Result<(), Self::BuildError> {
        Ok(())
    }
}

#[test]
#[ignore]
pub fn empty_install() {
    let mut system = LxcInstance::start(LxcInstance::DEFAULT_IMAGE);
    let dirs = Dirs::new("/server");
    SiDe::run_command(Command::Init, &dirs, &mut system, EmptyBuilder).unwrap();
    SiDe::run_command(
        Command::Build {
            ignore_verification: false,
            ask_overwrite: false,
        },
        &dirs,
        &mut system,
        EmptyBuilder,
    )
    .unwrap();
    SiDe::run_command(
        Command::Apply {
            target: 0,
            ignore_verification: false,
            ask_overwrite: false,
        },
        &dirs,
        &mut system,
        EmptyBuilder,
    )
    .unwrap();
    SiDe::run_command(
        Command::Apply {
            target: 1,
            ignore_verification: false,
            ask_overwrite: false,
        },
        &dirs,
        &mut system,
        EmptyBuilder,
    )
    .unwrap();
    SiDe::run_command(
        Command::Verify { fix: false },
        &dirs,
        &mut system,
        EmptyBuilder,
    )
    .unwrap();
}
