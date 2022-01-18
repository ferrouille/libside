use std::{
    ffi::CString,
    fs,
    io::{self, Read, Write},
    os::unix::prelude::PermissionsExt,
    path::Path,
    process::{Command, Stdio},
};

use etc_passwd::Passwd;

pub trait System: std::fmt::Debug {
    type Error: std::error::Error;
    type CommandError: std::error::Error;

    fn path_exists(&self, path: &Path) -> Result<bool, Self::Error>;

    fn file_contents(&self, path: &Path) -> Result<Vec<u8>, Self::Error>;

    fn execute_command(
        &self,
        path: &str,
        args: &[&str],
    ) -> Result<CommandResult, Self::CommandError>;

    fn execute_command_with_input(
        &self,
        path: &str,
        args: &[&str],
        input: &[u8],
    ) -> Result<CommandResult, Self::CommandError>;

    fn copy_file(&mut self, from: &Path, to: &Path) -> Result<(), Self::Error>;

    fn make_dir(&mut self, path: &Path) -> Result<(), Self::Error>;

    fn remove_dir(&mut self, path: &Path) -> Result<(), Self::Error>;

    fn remove_file(&mut self, path: &Path) -> Result<(), Self::Error>;

    fn get_user(&mut self, name: &str) -> Result<Option<()>, Self::Error>;

    fn chmod(&mut self, path: &Path, mode: u32) -> Result<(), Self::Error>;
}

#[derive(Debug)]
pub struct LocalSystem;

impl System for LocalSystem {
    type Error = io::Error;
    type CommandError = io::Error;

    fn path_exists(&self, path: &Path) -> Result<bool, Self::Error> {
        // TODO: Distinguish between directory and file
        match fs::symlink_metadata(path) {
            Ok(_) => Ok(true),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(false),
            Err(e) => Err(e),
        }
    }

    fn file_contents(&self, path: &Path) -> Result<Vec<u8>, Self::Error> {
        Ok(fs::read(path)?)
    }

    fn execute_command(&self, path: &str, args: &[&str]) -> Result<CommandResult, Self::Error> {
        let command = Command::new(path).args(args).output()?;

        Ok(CommandResult {
            exit_code: command.status.code(),
            stdout: command.stdout,
        })
    }

    fn copy_file(&mut self, from: &Path, to: &Path) -> Result<(), Self::Error> {
        fs::copy(from, to)?;
        Ok(())
    }

    fn make_dir(&mut self, path: &Path) -> Result<(), Self::Error> {
        fs::create_dir(path)?;
        Ok(())
    }

    fn remove_dir(&mut self, path: &Path) -> Result<(), Self::Error> {
        fs::remove_dir(path)?;
        Ok(())
    }

    fn remove_file(&mut self, path: &Path) -> Result<(), Self::Error> {
        fs::remove_file(path)?;
        Ok(())
    }

    fn get_user(&mut self, name: &str) -> Result<Option<()>, Self::Error> {
        Ok(Passwd::from_name(CString::new(name).unwrap())?.map(|_| ()))
    }

    fn execute_command_with_input(
        &self,
        path: &str,
        args: &[&str],
        input: &[u8],
    ) -> Result<CommandResult, Self::CommandError> {
        let mut child = Command::new(path)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()?;

        // TODO: This may propagate an error that we should handle (i.e. blocking reads return an error)
        let mut stdin_stream = child.stdin.take();
        let mut stdout_stream = child.stdout.take().unwrap();
        let mut stdout = Vec::new();
        let mut to_write = input;
        let mut buf = [0u8; 4096];
        let status = loop {
            match child.try_wait()? {
                Some(status) => break status,
                None => {
                    if let Some(stdin) = &mut stdin_stream {
                        if to_write.len() > 0 {
                            let written = stdin.write(&to_write)?;
                            println!(
                                "Written: [{}]",
                                std::str::from_utf8(&to_write[..written]).unwrap()
                            );
                            to_write = &to_write[written..];

                            if to_write.len() <= 0 {
                                let stdin = stdin_stream.take().unwrap();
                                drop(stdin);
                            }
                        }
                    }

                    let read = stdout_stream.read(&mut buf)?;
                    println!("Read: [{}]", std::str::from_utf8(&buf[..read]).unwrap());

                    if read == 0 {
                        break child.wait()?;
                    } else {
                        stdout.extend(&buf[..read]);
                    }
                }
            }
        };

        Ok(CommandResult {
            exit_code: status.code(),
            stdout,
        })
    }

    fn chmod(&mut self, path: &Path, mode: u32) -> Result<(), Self::Error> {
        let metadata = path.metadata()?;
        let mut permissions = metadata.permissions();
        permissions.set_mode(mode);
        fs::set_permissions(path, permissions)?;

        Ok(())
    }
}

pub struct CommandResult {
    stdout: Vec<u8>,
    exit_code: Option<i32>,
}

impl std::fmt::Debug for CommandResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CommandResult")
            .field("stdout", &self.stdout_as_str())
            .field("exit_code", &self.exit_code)
            .finish()
    }
}

impl CommandResult {
    pub fn is_success(&self) -> bool {
        self.exit_code == Some(0)
    }

    pub fn stdout_as_str(&self) -> &str {
        std::str::from_utf8(&self.stdout).unwrap()
    }
}

#[derive(Debug, thiserror::Error)]
#[error("This should never happen")]
pub struct NeverError;
