use std::path::PathBuf;

use libside::{
    builder::{
        fs::{ConfigFileData, CreateDirectory, FileWithContents},
        Builder,
    },
    requirements,
    system::System,
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
    type Requirement = requirements!(CreateDirectory, FileWithContents,);
    type BuildError = EmptyError;

    fn start_build(
        &self,
        context: &mut libside::builder::Context<Self::Requirement>,
    ) -> Result<Self::Data, Self::BuildError> {
        let dir = context.config_root().make_dir(context, "test");

        dir.make_file(
            context,
            ConfigFileData {
                path: PathBuf::from("message.txt"),
                contents: String::from("Hello, world!").into_bytes(),
                path_dependency: dir.graph_node(),
                extra_dependencies: Vec::new(),
            },
        );

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
pub fn single_file_install() {
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

    assert_eq!(
        system
            .file_contents(&PathBuf::from(
                "/server/files/config/_start/test/message.txt"
            ))
            .unwrap(),
        String::from("Hello, world!").into_bytes()
    );

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

    assert!(!system
        .path_exists(&PathBuf::from(
            "/server/files/config/_start/test/message.txt"
        ))
        .unwrap());

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

    assert_eq!(
        system
            .file_contents(&PathBuf::from(
                "/server/files/config/_start/test/message.txt"
            ))
            .unwrap(),
        String::from("Hello, world!").into_bytes()
    );

    system
        .remove_file(&PathBuf::from(
            "/server/files/config/_start/test/message.txt",
        ))
        .unwrap();

    let result = SiDe::run_command(
        Command::Verify { fix: false },
        &dirs,
        &mut system,
        EmptyBuilder,
    );
    assert!(result.is_err());
    assert!(!system
        .path_exists(&PathBuf::from(
            "/server/files/config/_start/test/message.txt"
        ))
        .unwrap());

    SiDe::run_command(
        Command::Verify { fix: true },
        &dirs,
        &mut system,
        EmptyBuilder,
    )
    .unwrap();

    assert_eq!(
        system
            .file_contents(&PathBuf::from(
                "/server/files/config/_start/test/message.txt"
            ))
            .unwrap(),
        String::from("Hello, world!").into_bytes()
    );
}
