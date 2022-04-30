use crate::system::{handle_process_io, System};
use lazy_static::lazy_static;
use rand::{distributions::Alphanumeric, prelude::*};
use std::{
    process::{Command, Stdio},
    sync::Mutex,
};

lazy_static! {
    static ref LOCK: Mutex<()> = Mutex::new(());
}

#[derive(Debug)]
pub struct LxcInstance {
    is_ready: bool,
    name: String,
}

impl LxcInstance {
    pub const DEFAULT_IMAGE: &'static str = Self::UBUNTU_FOCAL;
    pub const UBUNTU_FOCAL: &'static str = "images:ubuntu/focal";
    pub const UBUNTU_JAMMY: &'static str = "images:ubuntu/jammy";

    pub fn start(image: &str) -> LxcInstance {
        // Make sure we don't launch multiple VMs at the same time because that somehow causes crashes.
        let guard = LOCK.lock().unwrap();
        let name = format!(
            "side-test-{}",
            rand::thread_rng()
                .sample_iter(&Alphanumeric)
                .take(7)
                .map(char::from)
                .collect::<String>()
        );

        let result = Command::new("lxc")
            .arg("launch")
            .arg(image)
            .arg(&name)
            .arg("--vm")
            .arg("-p")
            .arg("default")
            .output()
            .unwrap();
        drop(guard);

        assert!(
            result.status.success(),
            "lxc launch failed: {}{}",
            std::str::from_utf8(&result.stdout).unwrap(),
            std::str::from_utf8(&result.stderr).unwrap()
        );

        println!("lxc instance started as {}", name);
        let mut inst = LxcInstance {
            name,
            is_ready: false,
        };
        inst.wait_until_ready();

        println!("Ready");
        inst
    }

    fn wait_until_ready(&mut self) {
        for _ in 0..100 {
            let result = Command::new("lxc")
                .arg("exec")
                .arg(&self.name)
                .arg("true")
                .output()
                .unwrap();
            if result.status.success() {
                return;
            }
        }

        self.is_ready = true;

        panic!("Container {} is not starting", self.name);
    }

    pub fn copy_files_to_container(&mut self, host_source: &str, container_target: &str) {
        Command::new("lxc")
            .arg("push")
            .arg(host_source)
            .arg(&format!(
                "{}/{}",
                self.name,
                container_target
                    .strip_prefix("/")
                    .expect("container target path must be absolute")
            ))
            .output()
            .unwrap();
    }
}

impl Drop for LxcInstance {
    fn drop(&mut self) {
        Command::new("lxc")
            .arg("delete")
            .arg("--force")
            .arg(&self.name)
            .output()
            .unwrap();
    }
}

#[derive(Debug, Clone, thiserror::Error)]
pub enum LxcError {
    #[error("path does not exist")]
    PathDoesNotExist,
}

impl System for LxcInstance {
    type Error = LxcError;
    type CommandError = LxcError;

    fn path_exists(&self, path: &std::path::Path) -> Result<bool, Self::Error> {
        let path = path.as_os_str().to_str().unwrap();
        let result = self.execute_command("/usr/bin/[", &["-e", path, "]"])?;

        Ok(result.is_success())
    }

    fn path_is_dir(&self, path: &std::path::Path) -> Result<bool, Self::Error> {
        let path = path.as_os_str().to_str().unwrap();
        let result = self.execute_command("/usr/bin/[", &["-d", path, "]"])?;

        Ok(result.is_success())
    }

    fn file_contents(&self, path: &std::path::Path) -> Result<Vec<u8>, Self::Error> {
        let path = path.as_os_str().to_str().unwrap();
        let result = self.execute_command("/usr/bin/cat", &[path])?;

        if result.is_success() {
            Ok(result.stdout().to_vec())
        } else {
            Err(LxcError::PathDoesNotExist)
        }
    }

    fn execute_command(
        &self,
        path: &str,
        args: &[&str],
    ) -> Result<crate::system::CommandResult, Self::CommandError> {
        self.execute_command_with_input(path, args, &[])
    }

    fn execute_command_with_input(
        &self,
        path: &str,
        args: &[&str],
        input: &[u8],
    ) -> Result<crate::system::CommandResult, Self::CommandError> {
        println!("Running command: {:?} {:?} {:?}", path, args, input);
        let child = Command::new("lxc")
            .arg("exec")
            .arg(&self.name)
            .arg("--")
            .arg(path)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        Ok(handle_process_io(child, input).unwrap())
    }

    fn copy_file(
        &mut self,
        from: &std::path::Path,
        to: &std::path::Path,
    ) -> Result<(), Self::Error> {
        let from = from.as_os_str().to_str().unwrap();
        let to = to.as_os_str().to_str().unwrap();
        let result = self.execute_command("/usr/bin/cp", &[from, to])?;

        assert!(result.is_success());

        Ok(())
    }

    fn make_dir(&mut self, path: &std::path::Path) -> Result<(), Self::Error> {
        let path = path.as_os_str().to_str().unwrap();
        let result = self.execute_command("/usr/bin/mkdir", &[path])?;

        assert!(result.is_success());

        Ok(())
    }

    fn remove_dir(&mut self, path: &std::path::Path) -> Result<(), Self::Error> {
        let path = path.as_os_str().to_str().unwrap();
        let result = self.execute_command("/usr/bin/rmdir", &[path])?;

        assert!(result.is_success());

        Ok(())
    }

    fn remove_file(&mut self, path: &std::path::Path) -> Result<(), Self::Error> {
        let path = path.as_os_str().to_str().unwrap();
        let result = self.execute_command("/usr/bin/rm", &[path])?;

        assert!(result.is_success());

        Ok(())
    }

    fn get_user(&mut self, name: &str) -> Result<Option<()>, Self::Error> {
        let result = self.execute_command("/usr/bin/id", &[name])?;

        Ok(if result.is_success() { Some(()) } else { None })
    }

    fn chmod(&mut self, path: &std::path::Path, mode: u32) -> Result<(), Self::Error> {
        let mode = format!("{:o}", mode);
        let path = path.as_os_str().to_str().unwrap();
        let result = self.execute_command("/usr/bin/chmod", &[&mode, path])?;

        assert!(result.is_success());

        Ok(())
    }

    fn put_file_contents(
        &self,
        path: &std::path::Path,
        contents: &[u8],
    ) -> Result<(), Self::Error> {
        let result =
            self.execute_command_with_input("/usr/bin/tee", &[path.to_str().unwrap()], contents)?;

        assert!(result.is_success());

        Ok(())
    }

    fn make_dir_all(&mut self, path: &std::path::Path) -> Result<(), Self::Error> {
        let path = path.as_os_str().to_str().unwrap();
        let result = self.execute_command("/usr/bin/mkdir", &["-p", path])?;

        assert!(result.is_success());

        Ok(())
    }

    fn read_dir(&mut self, path: &std::path::Path) -> Result<Vec<String>, Self::Error> {
        let result = self.execute_command("/usr/bin/ls", &[path.to_str().unwrap()])?;
        let output = result.stdout_as_str().trim();
        let data = if output == "" {
            Vec::new()
        } else {
            output.split('\n').map(|s| s.to_owned()).collect::<Vec<_>>()
        };

        Ok(data)
    }
}
