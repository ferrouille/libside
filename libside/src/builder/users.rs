use super::{AsParam, Context};
use crate::system::System;
use crate::utils::parse_etc_group;
use crate::{
    graph::GraphNodeReference,
    requirements::{Requirement, Supports},
    system::NeverError,
};
use itertools::Itertools;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs::File;
use std::iter::once;
use std::{fmt::Display, io::Cursor, path::PathBuf};

pub struct User {
    pub(crate) uid: Option<UserId>,
    pub(crate) name: String,
    pub(crate) node: GraphNodeReference,
}

pub struct UserId(u32);

impl Display for UserId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Default)]
struct MappedUsers {
    users: Vec<(String, u32)>,
}

impl User {
    pub fn add<'r, R: Requirement + Supports<CreateUser> + Supports<CreateGroup>>(
        context: &mut Context<R>,
        name: &str,
        create: impl for<'c> FnOnce(&'c mut UserConfig<'r>) -> &'c mut UserConfig<'r>,
    ) -> (User, Group) {
        let existing = crate::utils::parse_etc_passwd(File::open("/etc/passwd").unwrap()).unwrap();
        let mut info = UserConfig {
            system: true,
            supplementary_groups: Vec::new(),
        };

        create(&mut info);

        let info = info;
        let group = Group::add(context, name, info.system);
        let mapped: &mut MappedUsers = context.state();

        if mapped.users.iter().any(|(n, _)| n == name) {
            panic!("The user {name} cannot be added multiple times");
        }

        let uid = if let Some(u) = existing.iter().find(|user| user.name == name) {
            u.uid
        } else {
            let existing = mapped
                .users
                .iter()
                .map(|(_, uid)| *uid)
                .chain(existing.iter().map(|u| u.uid))
                .collect::<HashSet<_>>();
            if info.system {
                let mut uid = 999;
                loop {
                    if !existing.contains(&uid) {
                        break uid;
                    }

                    uid = uid
                        .checked_sub(1)
                        .expect("No more system user IDs available");
                }
            } else {
                let mut uid = 1000;
                loop {
                    if !existing.contains(&uid) {
                        break uid;
                    }

                    uid = uid + 1;
                }
            }
        };

        mapped.users.push((name.to_string(), uid));

        let node = context.add_node(
            CreateUser {
                uid,
                name: name.to_string(),
                group: group.name.to_string(),
                system: info.system,
                supplementary_groups: info
                    .supplementary_groups
                    .iter()
                    .map(|g| g.name.clone())
                    .collect(),
                shell: "/bin/false".to_owned(),
                home_dir: None,
            },
            info.supplementary_groups
                .iter()
                .map(|g| &g.node)
                .chain(once(&group.graph_node())),
        );

        (
            User {
                uid: Some(UserId(uid)),
                name: name.to_string(),
                node,
            },
            group,
        )
    }

    pub fn id(&self) -> &UserId {
        match &self.uid {
            Some(uid) => uid,
            None => panic!("The uid of {} is not known before applying", self.name),
        }
    }

    pub fn graph_node(&self) -> GraphNodeReference {
        self.node
    }
}

impl AsParam for User {
    fn as_param(&self) -> String {
        self.name.clone()
    }
}

impl AsParam for UserId {
    fn as_param(&self) -> String {
        self.0.to_string()
    }
}

#[derive(Default)]
struct MappedGroups {
    groups: Vec<(String, u32)>,
}

pub struct GroupId(u32);

impl Display for GroupId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

pub struct Group {
    pub(crate) gid: Option<GroupId>,
    pub(crate) name: String,
    pub(crate) node: GraphNodeReference,
}

impl Group {
    pub fn add<R: Requirement + Supports<CreateGroup>>(
        context: &mut Context<R>,
        name: &str,
        system: bool,
    ) -> Group {
        let existing = crate::utils::parse_etc_group(File::open("/etc/group").unwrap()).unwrap();
        let mapped: &mut MappedGroups = context.state();

        if mapped.groups.iter().any(|(n, _)| n == name) {
            panic!("The group {name} cannot be added multiple times");
        }

        let gid = if let Some(g) = existing.iter().find(|group| group.name == name) {
            g.gid
        } else {
            let existing = mapped
                .groups
                .iter()
                .map(|(_, gid)| *gid)
                .chain(existing.iter().map(|u| u.gid))
                .collect::<HashSet<_>>();
            if system {
                let mut gid = 999;
                loop {
                    if !existing.contains(&gid) {
                        break gid;
                    }

                    gid = gid
                        .checked_sub(1)
                        .expect("No more system group IDs available");
                }
            } else {
                let mut gid = 1000;
                loop {
                    if !existing.contains(&gid) {
                        break gid;
                    }

                    gid = gid + 1;
                }
            }
        };

        mapped.groups.push((name.to_string(), gid));

        Group {
            gid: Some(GroupId(gid)),
            name: name.to_owned(),
            node: context.add_node(
                CreateGroup {
                    gid,
                    name: name.to_string(),
                    system,
                },
                &[],
            ),
        }
    }

    pub fn id(&self) -> &GroupId {
        match &self.gid {
            Some(gid) => gid,
            None => panic!("The gid of {} is not known before applying", self.name),
        }
    }

    pub fn graph_node(&self) -> GraphNodeReference {
        self.node
    }
}

impl AsParam for Group {
    fn as_param(&self) -> String {
        self.name.clone()
    }
}

pub struct UserConfig<'r> {
    system: bool,
    supplementary_groups: Vec<&'r Group>,
}

impl<'r> UserConfig<'r> {
    pub fn system(&mut self, val: bool) -> &mut Self {
        self.system = val;
        self
    }

    pub fn add_group(&mut self, group: &'r Group) -> &mut Self {
        self.supplementary_groups.push(group);
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CreateUser {
    pub(crate) uid: u32,
    pub(crate) name: String,
    pub(crate) group: String,
    pub(crate) system: bool,
    pub(crate) supplementary_groups: Vec<String>,
    pub(crate) shell: String,
    pub(crate) home_dir: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum CreateUserError<S: System> {
    #[error("unable to execute useradd: {0}")]
    FailedToStart(S::CommandError),

    #[error("useradd failed: {0} {1}")]
    Unsuccessful(String, String),
}

impl<S: System> From<(&str, &str)> for CreateUserError<S> {
    fn from(output: (&str, &str)) -> Self {
        CreateUserError::Unsuccessful(output.0.to_string(), output.1.to_string())
    }
}

#[derive(Debug, thiserror::Error)]
#[error("unable to check if user exists: {0}")]
pub struct CheckUserError<S: System>(S::Error);

impl Requirement for CreateUser {
    type CreateError<S: System> = CreateUserError<S>;
    type ModifyError<S: System> = CreateUserError<S>;
    type DeleteError<S: System> = CreateUserError<S>;
    type HasBeenCreatedError<S: System> = CheckUserError<S>;

    fn create<S: crate::system::System>(&self, system: &mut S) -> Result<(), Self::CreateError<S>> {
        let group_str;
        let mut args = Vec::new();

        if self.system {
            args.push("--system");
        }

        if let Some(home_dir) = &self.home_dir {
            args.push("--create-home");
            args.push("--home-dir");
            args.push(home_dir.as_str());
        } else {
            args.push("--no-create-home");
        }

        if self.supplementary_groups.len() > 0 {
            group_str = self.supplementary_groups.iter().join(",");

            args.push("--groups");
            args.push(&group_str);
        }

        args.push("--shell");
        args.push(&self.shell);

        args.push("--uid");
        let uid = self.uid.to_string();
        args.push(&uid);

        args.push("--gid");
        args.push(&self.group);

        args.push(&self.name);

        let result = system
            .execute_command("useradd", &args)
            .map_err(CreateUserError::FailedToStart)?;
        result.successful()?;

        Ok(())
    }

    fn modify<S: crate::system::System>(&self, system: &mut S) -> Result<(), Self::ModifyError<S>> {
        let group_str;
        let mut args = Vec::new();

        if let Some(home_dir) = &self.home_dir {
            args.push("--home");
            args.push(&home_dir);
        }

        if self.supplementary_groups.len() > 0 {
            group_str = self.supplementary_groups.iter().join(",");

            args.push("--groups");
            args.push(&group_str);
        }

        args.push("--shell");
        args.push(&self.shell);

        args.push(&self.name);

        let result = system
            .execute_command("usermod", &args)
            .map_err(CreateUserError::FailedToStart)?;
        result.successful()?;

        Ok(())
    }

    fn delete<S: crate::system::System>(&self, system: &mut S) -> Result<(), Self::DeleteError<S>> {
        let result = system
            .execute_command("userdel", &[&self.name])
            .map_err(CreateUserError::FailedToStart)?;
        result.successful()?;

        Ok(())
    }

    fn has_been_created<S: crate::system::System>(
        &self,
        system: &mut S,
    ) -> Result<bool, Self::HasBeenCreatedError<S>> {
        Ok(system
            .get_user(&self.name)
            .map_err(CheckUserError)?
            .is_some())
    }

    fn affects(&self, other: &Self) -> bool {
        self.name == other.name
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
        let user = system.get_user(&self.name).unwrap();

        // TODO: Verify other properties of this user

        Ok(user.is_some())
    }

    const NAME: &'static str = "user";
}

impl Display for CreateUser {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "user({})", self.name)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateGroup {
    gid: u32,
    name: String,
    system: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum CreateGroupError<S: System> {
    #[error("unable to execute groupadd: {0}")]
    FailedToStart(S::CommandError),

    #[error("groupadd failed: {0} {1}")]
    Unsuccessful(String, String),
}

impl<S: System> From<(&str, &str)> for CreateGroupError<S> {
    fn from(output: (&str, &str)) -> Self {
        CreateGroupError::Unsuccessful(output.0.to_string(), output.1.to_string())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum CheckGroupError<S: System> {
    #[error("could not read /etc/group: {0}")]
    ReadFailed(S::Error),

    #[error("could not parse /etc/group: {0}")]
    ParseFailed(std::io::Error),
}

impl Requirement for CreateGroup {
    type CreateError<S: System> = CreateGroupError<S>;
    type ModifyError<S: System> = NeverError;
    type DeleteError<S: System> = CreateGroupError<S>;
    type HasBeenCreatedError<S: System> = CheckGroupError<S>;

    fn create<S: crate::system::System>(&self, system: &mut S) -> Result<(), Self::CreateError<S>> {
        let mut args = Vec::new();
        if self.system {
            args.push("--system");
        }

        args.push("--gid");
        let gid = self.gid.to_string();
        args.push(&gid);

        args.push(&self.name);

        let result = system
            .execute_command("groupadd", &args)
            .map_err(CreateGroupError::FailedToStart)?;

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
            .execute_command("groupdel", &[&self.name])
            .map_err(CreateGroupError::FailedToStart)?;

        result.successful()?;

        Ok(())
    }

    fn has_been_created<S: crate::system::System>(
        &self,
        system: &mut S,
    ) -> Result<bool, Self::HasBeenCreatedError<S>> {
        let file = system.file_contents(&PathBuf::from("/etc/group")).unwrap();
        let groups = parse_etc_group(Cursor::new(file)).unwrap();

        Ok(groups.iter().any(|g| g.name == self.name))
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
        false
    }

    fn verify<S: System>(&self, system: &mut S) -> Result<bool, ()> {
        Ok(self.has_been_created(system).unwrap())
    }

    const NAME: &'static str = "group";
}

impl Display for CreateGroup {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "group({})", self.name)
    }
}
