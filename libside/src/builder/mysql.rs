use itertools::Itertools;
use serde::{Deserialize, Serialize};
use std::{collections::HashSet, fmt::Display, path::PathBuf};

use super::apt::AptPackage;
use super::systemd::ServiceRunning;
use super::{
    path::{FromPackage, Path},
    systemd::SystemdService,
    Context, Group, User,
};
use crate::graph::GraphNodeReference;
use crate::requirements::{Requirement, Supports};
use crate::system::{NeverError, System};

pub struct Database {
    name: String,
    node: GraphNodeReference,
}

impl std::fmt::Display for Database {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.name)
    }
}

pub struct MySqlUser {
    name: String,
    node: GraphNodeReference,
}

impl std::fmt::Display for MySqlUser {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.name)
    }
}

pub struct MySqlService {
    service: SystemdService,
}

pub struct MariaDb {
    service: MySqlService,
    node: GraphNodeReference,
}

impl AptPackage for MariaDb {
    const NAME: &'static str = "mariadb-server";

    fn create(node: GraphNodeReference) -> Self {
        MariaDb {
            service: MySqlService {
                service: SystemdService::from_name_unchecked("mariadb", node, vec![node]),
            },
            node,
        }
    }

    fn graph_node(&self) -> GraphNodeReference {
        self.node
    }
}

impl MariaDb {
    pub fn binary(&self) -> Path<FromPackage> {
        Path {
            base: PathBuf::from("/usr/sbin/mariadb"),
            path: PathBuf::new(),
            loc: FromPackage,
            node: Some(self.graph_node()),
        }
    }

    pub fn default_service(&mut self) -> &mut MySqlService {
        &mut self.service
    }

    pub fn mysql_user(&self) -> User {
        User {
            uid: None,
            name: "mysql".to_owned(),
            node: self.graph_node(),
        }
    }

    pub fn mysql_group(&self) -> Group {
        Group {
            gid: None,
            name: "mysql".to_owned(),
            node: self.graph_node(),
        }
    }
}

pub struct RunningMySqlService<'a>(GraphNodeReference, &'a ());

impl MySqlService {
    pub fn run<R: Requirement>(&mut self, context: &mut Context<R>) -> RunningMySqlService
    where
        R: Supports<ServiceRunning>,
    {
        let node = ServiceRunning::restart(context, &self.service);
        RunningMySqlService(node, &())
    }
}

impl<'a> RunningMySqlService<'a> {
    pub fn create_database<R: Requirement>(&self, context: &mut Context<R>, name: &str) -> Database
    where
        R: Supports<CreateMySqlDatabase>,
    {
        let deps = [self.0];
        let node = context.add_node(CreateMySqlDatabase::new(name), &deps);
        Database {
            name: name.to_string(),
            node,
        }
    }

    pub fn create_user<R: Requirement>(
        &self,
        context: &mut Context<R>,
        name: &str,
        pass: &str,
    ) -> MySqlUser
    where
        R: Supports<CreateMySqlUser>,
    {
        let deps = [self.0];
        let node = context.add_node(CreateMySqlUser::new(name, pass), &deps);
        MySqlUser {
            name: name.to_string(),
            node,
        }
    }

    pub fn unix_socket(&self) -> Path<FromPackage> {
        Path {
            base: PathBuf::from("/"),
            path: PathBuf::from("var/run/mysqld/mysqld.sock"),
            loc: FromPackage,
            node: Some(self.0),
        }
    }
}

#[derive(Copy, Clone, Debug, Hash, PartialEq, Eq)]
pub enum Privilege {
    Alter,
    Create,
    Delete,
    Drop,
    GrantOption,
    Index,
    Insert,
    LockTables,
    Select,
    Update,
    ShowView,
    Trigger,
}

impl AsRef<str> for Privilege {
    fn as_ref(&self) -> &str {
        use Privilege::*;
        match self {
            Alter => "ALTER",
            Create => "CREATE",
            Delete => "DELETE",
            Drop => "DROP",
            GrantOption => "GRANT OPTION",
            Index => "INDEX",
            Insert => "INSERT",
            LockTables => "LOCK TABLES",
            Select => "SELECT",
            Update => "UPDATE",
            ShowView => "SHOW VIEW",
            Trigger => "TRIGGER",
        }
    }
}

impl PartialOrd for Privilege {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        self.as_ref().partial_cmp(other.as_ref())
    }
}

impl Ord for Privilege {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.as_ref().cmp(other.as_ref())
    }
}

impl MySqlUser {
    pub fn grant<R: Requirement>(
        &self,
        context: &mut Context<R>,
        privileges: HashSet<Privilege>,
        on: &Database,
    ) -> GraphNodeReference
    where
        R: Supports<CreateMySqlGrant>,
    {
        let privileges = privileges.iter().sorted().map(Privilege::as_ref).join(", ");
        context.add_node(
            CreateMySqlGrant::new(&self.name, on.name.to_string(), &privileges),
            &[self.node, on.node],
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CreateMySqlDatabase {
    name: String,
}

impl CreateMySqlDatabase {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum MySqlError<S: System> {
    #[error("unable to execute mysql: {0}")]
    FailedToStart(S::CommandError),

    #[error("mysql query '{query}' failed: {stdout}{stderr}")]
    Unsuccessful {
        query: String,
        stdout: String,
        stderr: String,
    },
}

impl Requirement for CreateMySqlDatabase {
    type CreateError<S: System> = MySqlError<S>;
    type ModifyError<S: System> = NeverError;
    type DeleteError<S: System> = NeverError;
    type HasBeenCreatedError<S: System> = MySqlError<S>;

    fn create<S: crate::system::System>(&self, system: &mut S) -> Result<(), Self::CreateError<S>> {
        let query = format!("CREATE DATABASE `{}`;", self.name);
        let result = system
            .execute_command_with_input("mysql", &[], query.as_bytes())
            .map_err(MySqlError::FailedToStart)?;
        result
            .successful()
            .map_err(|(stdout, stderr)| MySqlError::Unsuccessful {
                query,
                stdout: stdout.to_string(),
                stderr: stderr.to_string(),
            })?;

        Ok(())
    }

    fn modify<S: crate::system::System>(
        &self,
        _system: &mut S,
    ) -> Result<(), Self::ModifyError<S>> {
        Ok(())
    }

    fn delete<S: crate::system::System>(
        &self,
        _system: &mut S,
    ) -> Result<(), Self::DeleteError<S>> {
        unimplemented!()
    }

    fn has_been_created<S: crate::system::System>(
        &self,
        system: &mut S,
    ) -> Result<bool, Self::HasBeenCreatedError<S>> {
        let query = format!("SHOW DATABASES LIKE '{}';", self.name);
        let result = system
            .execute_command_with_input("mysql", &["--column-names=false"], query.as_bytes())
            .map_err(MySqlError::FailedToStart)?;
        result
            .successful()
            .map_err(|(stdout, stderr)| MySqlError::Unsuccessful {
                query,
                stdout: stdout.to_string(),
                stderr: stderr.to_string(),
            })?;

        return Ok(result.stdout_as_str().trim() == self.name);
    }

    fn affects(&self, other: &Self) -> bool {
        self.name == other.name
    }

    fn supports_modifications(&self) -> bool {
        false
    }
    fn can_undo(&self) -> bool {
        false
    }
    fn may_pre_exist(&self) -> bool {
        true
    }

    fn verify<S: System>(&self, system: &mut S) -> Result<bool, ()> {
        Ok(self.has_been_created(system).unwrap())
    }

    const NAME: &'static str = "mysql_database";
}

impl Display for CreateMySqlDatabase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "mysqldb({})", self.name)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CreateMySqlUser {
    name: String,
    pass: String,
}

impl CreateMySqlUser {
    pub fn new(name: &str, pass: &str) -> Self {
        Self {
            name: name.to_string(),
            pass: pass.to_string(),
        }
    }
}

impl Requirement for CreateMySqlUser {
    type CreateError<S: System> = MySqlError<S>;
    type ModifyError<S: System> = MySqlError<S>;
    type DeleteError<S: System> = MySqlError<S>;
    type HasBeenCreatedError<S: System> = MySqlError<S>;

    fn create<S: crate::system::System>(&self, system: &mut S) -> Result<(), Self::CreateError<S>> {
        // TODO: Escape username & password
        let query = format!(
            "CREATE USER '{}'@'localhost' IDENTIFIED BY '{}'; FLUSH PRIVILEGES;",
            self.name, self.pass
        );
        let result = system
            .execute_command_with_input("mysql", &[], query.as_bytes())
            .map_err(MySqlError::FailedToStart)?;
        result
            .successful()
            .map_err(|(stdout, stderr)| MySqlError::Unsuccessful {
                query,
                stdout: stdout.to_string(),
                stderr: stderr.to_string(),
            })?;

        Ok(())
    }

    fn modify<S: crate::system::System>(&self, system: &mut S) -> Result<(), Self::ModifyError<S>> {
        // TODO: Escape username & password
        let query = format!(
            "ALTER USER '{}'@'localhost' IDENTIFIED BY '{}'; FLUSH PRIVILEGES;",
            self.name, self.pass
        );
        let result = system
            .execute_command_with_input("mysql", &[], query.as_bytes())
            .map_err(MySqlError::FailedToStart)?;
        result
            .successful()
            .map_err(|(stdout, stderr)| MySqlError::Unsuccessful {
                query,
                stdout: stdout.to_string(),
                stderr: stderr.to_string(),
            })?;

        Ok(())
    }

    fn delete<S: crate::system::System>(&self, system: &mut S) -> Result<(), Self::DeleteError<S>> {
        let query = format!("DROP USER '{}'@'localhost'; FLUSH PRIVILEGES;", self.name);
        let result = system
            .execute_command_with_input("mysql", &[], query.as_bytes())
            .map_err(MySqlError::FailedToStart)?;
        result
            .successful()
            .map_err(|(stdout, stderr)| MySqlError::Unsuccessful {
                query,
                stdout: stdout.to_string(),
                stderr: stderr.to_string(),
            })?;

        Ok(())
    }

    fn has_been_created<S: crate::system::System>(
        &self,
        system: &mut S,
    ) -> Result<bool, Self::HasBeenCreatedError<S>> {
        let query = format!(
            "SELECT User FROM mysql.user WHERE User = '{}' AND Host = 'localhost';",
            self.name
        );
        let result = system
            .execute_command_with_input("mysql", &["--column-names=false"], query.as_bytes())
            .map_err(MySqlError::FailedToStart)?;
        result
            .successful()
            .map_err(|(stdout, stderr)| MySqlError::Unsuccessful {
                query,
                stdout: stdout.to_string(),
                stderr: stderr.to_string(),
            })?;

        return Ok(result.stdout_as_str().trim() == self.name);
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
        Ok(self.has_been_created(system).unwrap())
    }

    const NAME: &'static str = "mysql_user";
}

impl Display for CreateMySqlUser {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "mysqluser({})", self.name)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CreateMySqlGrant {
    user: String,
    database: String,
    privileges: String,
}

impl CreateMySqlGrant {
    pub fn new(name: &str, database: String, privileges: &str) -> Self {
        Self {
            user: name.to_string(),
            database: database.to_string(),
            privileges: privileges.to_string(),
        }
    }
}

impl Requirement for CreateMySqlGrant {
    type CreateError<S: System> = MySqlError<S>;
    type ModifyError<S: System> = MySqlError<S>;
    type DeleteError<S: System> = MySqlError<S>;
    type HasBeenCreatedError<S: System> = MySqlError<S>;

    fn create<S: crate::system::System>(&self, system: &mut S) -> Result<(), Self::CreateError<S>> {
        let query = format!(
            "GRANT {p} ON `{db}`.* TO '{u}'@'localhost'; FLUSH PRIVILEGES;",
            p = self.privileges,
            db = self.database,
            u = self.user
        );
        let result = system
            .execute_command_with_input("mysql", &[], query.as_bytes())
            .map_err(MySqlError::FailedToStart)?;
        result
            .successful()
            .map_err(|(stdout, stderr)| MySqlError::Unsuccessful {
                query,
                stdout: stdout.to_string(),
                stderr: stderr.to_string(),
            })?;

        Ok(())
    }

    fn modify<S: crate::system::System>(&self, system: &mut S) -> Result<(), Self::ModifyError<S>> {
        let query = format!("REVOKE ALL PRIVILEGES ON `{db}`.* FROM '{u}'@'localhost'; GRANT {p} ON `{db}`.* TO '{u}'@'localhost'; FLUSH PRIVILEGES; FLUSH PRIVILEGES;", p = self.privileges, db = self.database, u = self.user);
        let result = system
            .execute_command_with_input("mysql", &[], query.as_bytes())
            .map_err(MySqlError::FailedToStart)?;
        result
            .successful()
            .map_err(|(stdout, stderr)| MySqlError::Unsuccessful {
                query,
                stdout: stdout.to_string(),
                stderr: stderr.to_string(),
            })?;

        Ok(())
    }

    fn delete<S: crate::system::System>(&self, system: &mut S) -> Result<(), Self::DeleteError<S>> {
        let query = format!(
            "REVOKE ALL PRIVILEGES ON `{db}`.* FROM '{u}'@'localhost'; FLUSH PRIVILEGES;",
            db = self.database,
            u = self.user
        );
        let result = system
            .execute_command_with_input("mysql", &[], query.as_bytes())
            .map_err(MySqlError::FailedToStart)?;
        result
            .successful()
            .map_err(|(stdout, stderr)| MySqlError::Unsuccessful {
                query,
                stdout: stdout.to_string(),
                stderr: stderr.to_string(),
            })?;

        Ok(())
    }

    fn has_been_created<S: crate::system::System>(
        &self,
        system: &mut S,
    ) -> Result<bool, Self::HasBeenCreatedError<S>> {
        let query = format!("SHOW GRANTS FOR {}@'localhost'", self.user);
        let result = system
            .execute_command_with_input("mysql", &["--column-names=false"], query.as_bytes())
            .unwrap();
        if !result.is_success() && result.stderr_as_str().contains("ERROR 1141 (42000)") {
            // ERROR 1141 (42000) at line 1: There is no such grant defined for user ...
            return Ok(false);
        }

        assert!(result.is_success()); // TODO

        let grants = result.stdout_as_str();
        println!("Grants: {}", grants);

        Ok(grants.contains(&format!(
            "ON `{}`.* TO `{}`@`localhost`",
            self.database, self.user
        )))
    }

    fn affects(&self, other: &Self) -> bool {
        self.user == other.user
    }

    fn supports_modifications(&self) -> bool {
        true
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

    const NAME: &'static str = "mysql_grant";
}

impl Display for CreateMySqlGrant {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "grant({})", self.user)
    }
}

#[cfg(test)]
mod tests {
    use crate::{builder::mysql::{CreateMySqlDatabase, CreateMySqlGrant, CreateMySqlUser}, testing::LxcInstance, requirements::Requirement, system::System};

    #[test]
    pub fn serialize_deserialize_create_mysql_database() {
        let r = CreateMySqlDatabase {
            name: String::from("foo"),
        };
        let json = r#"{"name":"foo"}"#;

        assert_eq!(serde_json::to_string(&r).unwrap(), json);
        assert_eq!(r, serde_json::from_str(json).unwrap());
    }

    #[test]
    #[ignore]
    pub fn lxc_create_mysql_database() {
        let mut sys = LxcInstance::start();
        let p = CreateMySqlDatabase {
            name: String::from("foo"),
        };

        sys.execute_command("apt-get", &[ "install", "-y", "mariadb-server" ]).unwrap();

        assert!(!p.has_been_created(&mut sys).unwrap());
        assert!(!p.verify(&mut sys).unwrap());

        p.create(&mut sys).unwrap();

        assert!(p.has_been_created(&mut sys).unwrap());
        assert!(p.verify(&mut sys).unwrap());

        // p.delete(&mut sys).unwrap();

        // assert!(!p.has_been_created(&mut sys).unwrap());
        // assert!(!p.verify(&mut sys).unwrap());
    }

    #[test]
    pub fn serialize_deserialize_create_mysql_user() {
        let r = CreateMySqlUser {
            name: String::from("foo"),
            pass: String::from("bar"),
        };
        let json = r#"{"name":"foo","pass":"bar"}"#;

        assert_eq!(serde_json::to_string(&r).unwrap(), json);
        assert_eq!(r, serde_json::from_str(json).unwrap());
    }

    #[test]
    #[ignore]
    pub fn lxc_create_mysql_user() {
        let mut sys = LxcInstance::start();
        let p = CreateMySqlUser {
            name: String::from("foo"),
            pass: String::from("bar"),
        };

        sys.execute_command("apt-get", &[ "install", "-y", "mariadb-server" ]).unwrap();

        assert!(!p.has_been_created(&mut sys).unwrap());
        assert!(!p.verify(&mut sys).unwrap());

        p.create(&mut sys).unwrap();

        assert!(p.has_been_created(&mut sys).unwrap());
        assert!(p.verify(&mut sys).unwrap());

        assert!(sys.execute_command_with_input("mysql", &["-ufoo", "-pbar"], "SELECT 1;".as_bytes()).unwrap().is_success(), "User was not created correctly");

        p.delete(&mut sys).unwrap();

        assert!(!p.has_been_created(&mut sys).unwrap());
        assert!(!p.verify(&mut sys).unwrap());
    }

    #[test]
    pub fn serialize_deserialize_create_mysql_grant() {
        let r = CreateMySqlGrant {
            user: String::from("foo"),
            database: String::from("bar"),
            privileges: String::from("baz"),
        };
        let json = r#"{"user":"foo","database":"bar","privileges":"baz"}"#;

        assert_eq!(serde_json::to_string(&r).unwrap(), json);
        assert_eq!(r, serde_json::from_str(json).unwrap());
    }


    #[test]
    #[ignore]
    pub fn lxc_create_mysql_grant() {
        let mut sys = LxcInstance::start();
        let pre1 = CreateMySqlUser {
            name: String::from("foo"),
            pass: String::from("bar"),
        };
        let pre2 = CreateMySqlDatabase {
            name: String::from("baz"),
        };
        let p = CreateMySqlGrant {
            user: String::from("foo"),
            database: String::from("baz"),
            privileges: String::from("SELECT"),
        };

        sys.execute_command("apt-get", &[ "install", "-y", "mariadb-server" ]).unwrap();

        // Check when user and db don't exist
        assert!(!p.has_been_created(&mut sys).unwrap());
        assert!(!p.verify(&mut sys).unwrap());

        pre1.create(&mut sys).unwrap();

        // Check when only user exists
        assert!(!p.has_been_created(&mut sys).unwrap());
        assert!(!p.verify(&mut sys).unwrap());

        pre1.delete(&mut sys).unwrap();
        pre2.create(&mut sys).unwrap();

        // Check when only db exists
        assert!(!p.has_been_created(&mut sys).unwrap());
        assert!(!p.verify(&mut sys).unwrap());

        pre1.create(&mut sys).unwrap();

        // Check when both db and user exist
        assert!(!p.has_been_created(&mut sys).unwrap());
        assert!(!p.verify(&mut sys).unwrap());

        p.create(&mut sys).unwrap();

        assert!(p.has_been_created(&mut sys).unwrap());
        assert!(p.verify(&mut sys).unwrap());

        p.delete(&mut sys).unwrap();

        assert!(!p.has_been_created(&mut sys).unwrap());
        assert!(!p.verify(&mut sys).unwrap());
    }
}
