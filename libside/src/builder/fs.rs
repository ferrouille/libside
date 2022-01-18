use crate::builder::GeneratedFile;
use crate::graph::GraphNodeReference;
use crate::requirements::{Requirement, Supports};
use crate::system::{NeverError, System};
use serde::{Deserialize, Serialize};
use sha3::{Digest, Sha3_256};
use std::path::Path as StdPath;
use std::{fmt::Display, path::PathBuf};

use super::path::{CanWritePath, Path, WillBeCreated};
use super::Context;

pub struct ConfigFileData {
    pub path: PathBuf,
    pub contents: Vec<u8>,
    pub path_dependency: Option<GraphNodeReference>,
    pub extra_dependencies: Vec<GraphNodeReference>,
}

impl ConfigFileData {
    pub fn path(&self) -> &StdPath {
        self.path.as_ref()
    }

    pub fn contents(self) -> Vec<u8> {
        self.contents.into()
    }

    pub fn path_dependency<'a>(&'a self) -> Option<GraphNodeReference> {
        self.path_dependency.clone()
    }

    pub fn extra_dependencies<'a>(&'a self) -> std::slice::Iter<'a, GraphNodeReference> {
        self.extra_dependencies.iter()
    }

    pub fn in_dir<L: Clone + CanWritePath>(self, dir: &Path<L>) -> ConfigFileData {
        ConfigFileData {
            path: dir.full_path().join(self.path()),
            path_dependency: dir.node.clone(),
            extra_dependencies: self.extra_dependencies().copied().collect(),
            contents: self.contents(),
        }
    }

    pub fn set_full_path<L: Clone + CanWritePath>(self, path: &Path<L>) -> ConfigFileData {
        ConfigFileData {
            path: path.full_path(),
            path_dependency: path.node.clone(),
            extra_dependencies: self.extra_dependencies().copied().collect(),
            contents: self.contents(),
        }
    }

    pub fn rename<S: AsRef<StdPath>>(self, new_name: S) -> ConfigFileData {
        let path = self
            .path()
            .parent()
            .unwrap_or(&PathBuf::new())
            .join(new_name);
        ConfigFileData {
            path,
            path_dependency: self.path_dependency(),
            extra_dependencies: self.extra_dependencies().copied().collect(),
            contents: self.contents(),
        }
    }

    pub fn create<R: Requirement + Supports<FileWithContents>>(
        self,
        context: &mut Context<R>,
    ) -> Path<WillBeCreated> {
        let path = self.path().to_path_buf();
        assert!(path.is_absolute());
        let source = context.generated_path.join(path.strip_prefix("/").unwrap());

        let depends_on = self
            .path_dependency()
            .iter()
            .chain(self.extra_dependencies())
            .copied()
            .collect::<Vec<_>>();
        let contents = self.contents();
        let node = context.add_node(
            FileWithContents::new(source.clone(), path.clone(), Sha3::hash(&contents)),
            &depends_on,
        );

        context.files.push(GeneratedFile {
            source,
            contents,
            needs_cleanup: true,
        });

        Path {
            base: path,
            path: PathBuf::new(),
            loc: WillBeCreated,
            node: Some(node),
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct Sha3([u8; 32]);

impl Sha3 {
    pub fn hash(bytes: &[u8]) -> Sha3 {
        let mut hasher = Sha3_256::new();
        hasher.update(bytes);

        Sha3(hasher.finalize().into())
    }
}

impl Display for Sha3 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for b in self.0.iter() {
            write!(f, "{:02x}", b)?;
        }

        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileWithContents {
    local_file: PathBuf,
    to: PathBuf,
    sha3: Sha3,
}

#[derive(Debug, Clone, thiserror::Error)]
#[error("Unable to copy file from {} to {}: {}", from.display(), to.display(), inner)]
pub struct FileCreateError<S: System> {
    from: PathBuf,
    to: PathBuf,
    inner: S::Error,
}

#[derive(Debug, Clone, thiserror::Error)]
#[error("Unable to delete file {}: {}", path.display(), inner)]
pub struct FileDeleteError<S: System> {
    path: PathBuf,
    inner: S::Error,
}

impl Requirement for FileWithContents {
    type CreateError<S: System> = FileCreateError<S>;
    type ModifyError<S: System> = FileCreateError<S>;
    type DeleteError<S: System> = FileDeleteError<S>;
    type HasBeenCreatedError<S: System> = S::Error;

    fn create<S: crate::system::System>(&self, system: &mut S) -> Result<(), Self::CreateError<S>> {
        println!("  create: {}", self.to.display());
        system
            .copy_file(&self.local_file, &self.to)
            .map_err(|inner| FileCreateError {
                from: self.local_file.clone(),
                to: self.to.clone(),
                inner,
            })
    }

    fn modify<S: crate::system::System>(&self, system: &mut S) -> Result<(), Self::ModifyError<S>> {
        println!("  update: {}", self.to.display());
        system
            .copy_file(&self.local_file, &self.to)
            .map_err(|inner| FileCreateError {
                from: self.local_file.clone(),
                to: self.to.clone(),
                inner,
            })
    }

    fn delete<S: crate::system::System>(&self, system: &mut S) -> Result<(), Self::DeleteError<S>> {
        println!("  delete: {}", self.to.display());
        system
            .remove_file(&self.to)
            .map_err(|inner| FileDeleteError {
                path: self.to.clone(),
                inner,
            })
    }

    fn has_been_created<S: crate::system::System>(
        &self,
        system: &mut S,
    ) -> Result<bool, Self::HasBeenCreatedError<S>> {
        system.path_exists(&self.to)
    }

    fn affects(&self, other: &Self) -> bool {
        self.to == other.to
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
        Ok(if self.has_been_created(system).unwrap() {
            let contents = system.file_contents(&self.to).unwrap();
            Sha3::hash(&contents) == self.sha3
        } else {
            false
        })
    }

    const NAME: &'static str = "file_with_contents";
}

impl FileWithContents {
    pub fn new(source: PathBuf, to: PathBuf, sha3: Sha3) -> Self {
        Self {
            local_file: source,
            to,
            sha3,
        }
    }
}

impl Display for FileWithContents {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "file({})", self.to.display())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateDirectory {
    path: PathBuf,
    needs_cleanup: bool,
}

impl CreateDirectory {
    pub fn new(path: PathBuf) -> CreateDirectory {
        CreateDirectory {
            path,
            needs_cleanup: true,
        }
    }

    pub fn new_without_cleanup(path: PathBuf) -> CreateDirectory {
        CreateDirectory {
            path,
            needs_cleanup: false,
        }
    }
}

#[derive(Debug, thiserror::Error)]
#[error("{} could not be created: {}", path.display(), inner)]
pub struct DirectoryCreateError<S: System> {
    path: PathBuf,
    inner: S::Error,
}

#[derive(Debug, thiserror::Error)]
#[error("{} could not be deleted: {}", path.display(), inner)]
pub struct DirectoryDeleteError<S: System> {
    path: PathBuf,
    inner: S::Error,
}

impl Requirement for CreateDirectory {
    type CreateError<S: System> = DirectoryCreateError<S>;
    type ModifyError<S: System> = NeverError;
    type DeleteError<S: System> = DirectoryDeleteError<S>;
    type HasBeenCreatedError<S: System> = S::Error;

    fn create<S: crate::system::System>(&self, system: &mut S) -> Result<(), Self::CreateError<S>> {
        // TODO: use system instead

        println!("  mkdir: {}", self.path.display());
        system
            .make_dir(&self.path)
            .map_err(|inner| DirectoryCreateError {
                path: self.path.clone(),
                inner,
            })
    }

    fn modify<S: crate::system::System>(
        &self,
        _system: &mut S,
    ) -> Result<(), Self::ModifyError<S>> {
        Ok(())
    }

    fn delete<S: crate::system::System>(&self, system: &mut S) -> Result<(), Self::DeleteError<S>> {
        println!("  rmdir: {}", self.path.display());
        system
            .remove_dir(&self.path)
            .map_err(|inner| DirectoryDeleteError {
                path: self.path.clone(),
                inner,
            })
    }

    fn has_been_created<S: crate::system::System>(
        &self,
        system: &mut S,
    ) -> Result<bool, Self::HasBeenCreatedError<S>> {
        Ok(system.path_exists(&self.path)?)
    }

    fn affects(&self, other: &Self) -> bool {
        self.path == other.path
    }

    fn supports_modifications(&self) -> bool {
        false
    }
    fn can_undo(&self) -> bool {
        self.needs_cleanup
    }
    fn may_pre_exist(&self) -> bool {
        !self.needs_cleanup
    }

    fn verify<S: System>(&self, system: &mut S) -> Result<bool, ()> {
        Ok(self.has_been_created(system).unwrap())
    }

    const NAME: &'static str = "directory";
}

impl Display for CreateDirectory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "dir({})", self.path.display())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Delete {
    path: PathBuf,
    copy_to: PathBuf,
}

impl Delete {
    pub fn new(path: PathBuf, copy_to: PathBuf) -> Delete {
        Delete { path, copy_to }
    }
}

impl Requirement for Delete {
    type CreateError<S: System> = NeverError;
    type ModifyError<S: System> = NeverError;
    type DeleteError<S: System> = NeverError;
    type HasBeenCreatedError<S: System> = NeverError;

    fn create<S: crate::system::System>(&self, system: &mut S) -> Result<(), Self::CreateError<S>> {
        println!("  del: {}", self.path.display());
        system.copy_file(&self.path, &self.copy_to).unwrap();
        system.remove_file(&self.path).unwrap();

        Ok(())
    }

    fn modify<S: crate::system::System>(
        &self,
        _system: &mut S,
    ) -> Result<(), Self::ModifyError<S>> {
        Ok(())
    }

    fn delete<S: crate::system::System>(&self, system: &mut S) -> Result<(), Self::DeleteError<S>> {
        println!("  undel: {}", self.path.display());
        system.copy_file(&self.copy_to, &self.path).unwrap();
        system.remove_file(&self.copy_to).unwrap();
        Ok(())
    }

    fn has_been_created<S: crate::system::System>(
        &self,
        system: &mut S,
    ) -> Result<bool, Self::HasBeenCreatedError<S>> {
        Ok(!system.path_exists(&self.path).unwrap())
    }

    fn affects(&self, other: &Self) -> bool {
        self.path == other.path
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

    const NAME: &'static str = "delete";
}

impl Display for Delete {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "deleted({})", self.path.display())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Chown {
    path: PathBuf,
    user: String,
    group: String,
}

impl Chown {
    pub fn new(path: PathBuf, user: String, group: String) -> Chown {
        Chown {
            path,
            user: user,
            group: group,
        }
    }
}

impl Requirement for Chown {
    type CreateError<S: System> = NeverError;
    type ModifyError<S: System> = NeverError;
    type DeleteError<S: System> = NeverError;
    type HasBeenCreatedError<S: System> = NeverError;

    fn create<S: crate::system::System>(&self, system: &mut S) -> Result<(), Self::CreateError<S>> {
        system
            .execute_command(
                "/usr/bin/chown",
                &[
                    &format!("{}:{}", self.user, self.group),
                    self.path.as_os_str().to_str().unwrap(),
                ],
            )
            .unwrap();

        Ok(())
    }

    fn modify<S: crate::system::System>(&self, system: &mut S) -> Result<(), Self::ModifyError<S>> {
        self.create(system)
    }

    fn delete<S: crate::system::System>(
        &self,
        _system: &mut S,
    ) -> Result<(), Self::DeleteError<S>> {
        Ok(())
    }

    fn has_been_created<S: crate::system::System>(
        &self,
        _system: &mut S,
    ) -> Result<bool, Self::HasBeenCreatedError<S>> {
        Ok(true)
    }

    fn affects(&self, other: &Self) -> bool {
        self.path == other.path
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

    const NAME: &'static str = "chown";
}

impl Display for Chown {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "chown({})", self.path.display())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Chmod {
    path: PathBuf,
    permissions: u32,
}

impl Chmod {
    pub fn new(path: PathBuf, permissions: u32) -> Chmod {
        Chmod { path, permissions }
    }
}

impl Requirement for Chmod {
    type CreateError<S: System> = NeverError;
    type ModifyError<S: System> = NeverError;
    type DeleteError<S: System> = NeverError;
    type HasBeenCreatedError<S: System> = NeverError;

    fn create<S: crate::system::System>(&self, system: &mut S) -> Result<(), Self::CreateError<S>> {
        system.chmod(&self.path, self.permissions).unwrap();

        Ok(())
    }

    fn modify<S: crate::system::System>(&self, system: &mut S) -> Result<(), Self::ModifyError<S>> {
        self.create(system)
    }

    fn delete<S: crate::system::System>(
        &self,
        _system: &mut S,
    ) -> Result<(), Self::DeleteError<S>> {
        Ok(())
    }

    fn has_been_created<S: crate::system::System>(
        &self,
        _system: &mut S,
    ) -> Result<bool, Self::HasBeenCreatedError<S>> {
        Ok(true)
    }

    fn affects(&self, other: &Self) -> bool {
        self.path == other.path
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

    const NAME: &'static str = "chmod";
}

impl Display for Chmod {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "chmod({}, {:o})", self.path.display(), self.permissions)
    }
}
