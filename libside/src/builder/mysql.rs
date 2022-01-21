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

pub struct MariaDb(GraphNodeReference);

impl AptPackage for MariaDb {
    const NAME: &'static str = "mariadb-server";

    fn create(node: GraphNodeReference) -> Self {
        MariaDb(node)
    }

    fn graph_node(&self) -> GraphNodeReference {
        self.0
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

    pub fn default_service(&self) -> MySqlService {
        MySqlService {
            service: SystemdService::from_name_unchecked(
                "mariadb",
                self.graph_node(),
                vec![self.graph_node()],
            ),
        }
    }

    pub fn mysql_user(&self) -> User {
        User {
            name: "mysql".to_owned(),
            node: self.graph_node(),
        }
    }

    pub fn mysql_group(&self) -> Group {
        Group {
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

#[derive(Debug, Clone, Serialize, Deserialize)]
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

impl Requirement for CreateMySqlDatabase {
    type CreateError<S: System> = NeverError;
    type ModifyError<S: System> = NeverError;
    type DeleteError<S: System> = NeverError;
    type HasBeenCreatedError<S: System> = NeverError;

    fn create<S: crate::system::System>(&self, system: &mut S) -> Result<(), Self::CreateError<S>> {
        let query = format!("CREATE DATABASE `{}`;", self.name);
        let result = system
            .execute_command_with_input("mysql", &[], query.as_bytes())
            .unwrap();
        assert!(result.is_success()); // TODO

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
            .unwrap();
        assert!(result.is_success()); // TODO

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

#[derive(Debug, Clone, Serialize, Deserialize)]
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
    type CreateError<S: System> = NeverError;
    type ModifyError<S: System> = NeverError;
    type DeleteError<S: System> = NeverError;
    type HasBeenCreatedError<S: System> = NeverError;

    fn create<S: crate::system::System>(&self, system: &mut S) -> Result<(), Self::CreateError<S>> {
        // TODO: Escape username & password
        let query = format!(
            "CREATE USER '{}'@'localhost' IDENTIFIED BY '{}'; FLUSH PRIVILEGES;",
            self.name, self.pass
        );
        let result = system
            .execute_command_with_input("mysql", &[], query.as_bytes())
            .unwrap();
        assert!(result.is_success()); // TODO

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
            .unwrap();
        assert!(result.is_success()); // TODO

        Ok(())
    }

    fn delete<S: crate::system::System>(&self, system: &mut S) -> Result<(), Self::DeleteError<S>> {
        let query = format!("DROP USER '{}'@'localhost'; FLUSH PRIVILEGES;", self.name);
        let result = system
            .execute_command_with_input("mysql", &[], query.as_bytes())
            .unwrap();
        assert!(result.is_success()); // TODO

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
            .unwrap();
        assert!(result.is_success()); // TODO

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

#[derive(Debug, Clone, Serialize, Deserialize)]
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
    type CreateError<S: System> = NeverError;
    type ModifyError<S: System> = NeverError;
    type DeleteError<S: System> = NeverError;
    type HasBeenCreatedError<S: System> = NeverError;

    fn create<S: crate::system::System>(&self, system: &mut S) -> Result<(), Self::CreateError<S>> {
        let query = format!(
            "GRANT {p} ON `{db}`.* TO '{u}'@'localhost'; FLUSH PRIVILEGES;",
            p = self.privileges,
            db = self.database,
            u = self.user
        );
        let result = system
            .execute_command_with_input("mysql", &[], query.as_bytes())
            .unwrap();
        assert!(result.is_success()); // TODO

        Ok(())
    }

    fn modify<S: crate::system::System>(&self, system: &mut S) -> Result<(), Self::ModifyError<S>> {
        let query = format!("REVOKE ALL PRIVILEGES ON `{db}`.* FROM '{u}'@'localhost'; GRANT {p} ON `{db}`.* TO '{u}'@'localhost'; FLUSH PRIVILEGES; FLUSH PRIVILEGES;", p = self.privileges, db = self.database, u = self.user);
        let result = system
            .execute_command_with_input("mysql", &[], query.as_bytes())
            .unwrap();
        assert!(result.is_success()); // TODO

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
            .unwrap();
        assert!(result.is_success()); // TODO

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
