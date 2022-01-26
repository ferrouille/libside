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
    name: String,
}

impl LxcInstance {
    pub fn start() -> LxcInstance {
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
            .arg("images:ubuntu/focal")
            .arg(&name)
            .arg("--vm")
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
        let inst = LxcInstance { name };
        inst.wait_until_ready();

        println!("Ready");
        inst
    }

    fn wait_until_ready(&self) {
        for _ in 0..10 {
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

        panic!("Container {} is not starting", self.name);
    }
}

impl Drop for LxcInstance {
    fn drop(&mut self) {
        Command::new("lxc")
            .arg("stop")
            .arg(&self.name)
            .output()
            .unwrap();

        Command::new("lxc")
            .arg("delete")
            .arg(&self.name)
            .output()
            .unwrap();
    }
}

#[derive(Debug, Clone, thiserror::Error)]
#[error("Error.")]
pub struct LxcError;

impl System for LxcInstance {
    type Error = LxcError;
    type CommandError = LxcError;

    fn path_exists(&self, path: &std::path::Path) -> Result<bool, Self::Error> {
        let path = path.as_os_str().to_str().unwrap();
        let result = self.execute_command("/usr/bin/[", &["-e", path, "]"])?;

        Ok(result.is_success())
    }

    fn file_contents(&self, path: &std::path::Path) -> Result<Vec<u8>, Self::Error> {
        let path = path.as_os_str().to_str().unwrap();
        let result = self.execute_command("/usr/bin/cat", &[path])?;

        Ok(result.stdout().to_vec())
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
}
