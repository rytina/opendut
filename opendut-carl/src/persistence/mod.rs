use std::sync::{Mutex, MutexGuard};

use crate::resources::storage::volatile::VolatileResourcesStorage;
use diesel::PgConnection;

pub mod database;
pub(crate) mod resources;
mod query;

pub struct Storage<'a> {
    pub db: Db<'a>,
    pub memory: &'a mut Memory,
}
pub struct Db<'a> {
    pub inner: Mutex<&'a mut PgConnection>, //Mutex rather than RwLock, because we share this between threads (i.e. we need it to implement `Sync`)
}
impl<'a> Db<'a> {
    pub fn from_connection(connection: &'a mut PgConnection) -> Db {
        Self { inner: Mutex::new(connection) }
    }
    pub fn connection(&self) -> MutexGuard<&'a mut PgConnection> {
        self.inner.lock().expect("error while locking mutex for database connection")
    }
}
pub type Memory = VolatileResourcesStorage;

pub(crate) mod error {
    use std::fmt::{Display, Formatter};
    use uuid::Uuid;

    #[derive(Debug, thiserror::Error)]
    pub enum PersistenceError {
        Custom {
            resource_name: &'static str,
            operation: PersistenceOperation,
            context_messages: Vec<String>,
            id: Option<Uuid>,
            #[source] source: Option<Cause>,
        },
        DieselInternal {
            #[from] source: diesel::result::Error,
        },
    }
    impl PersistenceError {
        pub fn insert<R>(id: impl Into<Uuid>, cause: impl Into<Cause>) -> Self {
            Self::new::<R>(Some(id.into()), PersistenceOperation::Insert, Some(cause))
        }
        pub fn remove<R>(id: impl Into<Uuid>, cause: impl Into<Cause>) -> Self {
            Self::new::<R>(Some(id.into()), PersistenceOperation::Remove, Some(cause))
        }
        pub fn get<R>(id: impl Into<Uuid>, cause: impl Into<Cause>) -> Self {
            Self::new::<R>(Some(id.into()), PersistenceOperation::Get, Some(cause))
        }
        pub fn list<R>(cause: impl Into<Cause>) -> Self {
            Self::new::<R>(Option::<Uuid>::None, PersistenceOperation::List, Some(cause))
        }
        pub fn new<R>(id: Option<impl Into<Uuid>>, operation: PersistenceOperation, cause: Option<impl Into<Cause>>) -> Self {
            let id = id.map(Into::into);
            Self::Custom {
                resource_name: std::any::type_name::<R>(),
                operation,
                context_messages: Vec::new(),
                id,
                source: cause.map(Into::into),
            }
        }

        pub fn context(mut self, message: impl Into<String>) -> Self {
            match &mut self {
                PersistenceError::Custom { context_messages, .. } => context_messages.push(message.into()),
                PersistenceError::DieselInternal { .. } => unimplemented!(),
            }
            self
        }
    }
    impl Display for PersistenceError {
        fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
            match self {
                Self::Custom { resource_name, operation, context_messages, id, source } => {
                    let id = match &id {
                        Some(id) => format!(" <{id}>"),
                        None => String::from(""),
                    };
                    let operation = operation.verb();
                    writeln!(f, "Error while {operation} resource '{resource_name}'{id}")?;

                    for message in context_messages {
                        writeln!(f, "  Context: {message}")?;
                    }
                    source.as_ref().map(|source|
                        writeln!(f, "  Source: {source}")
                    ).transpose()?;
                }
                PersistenceError::DieselInternal { source } => writeln!(f, "Error internal to Diesel, likely from transaction: {source}")?,
            }
            Ok(())
        }
    }

    type Cause = Box<dyn std::error::Error + Send + Sync>;

    #[derive(Clone, Copy, Debug, PartialEq)]
    pub enum PersistenceOperation {
        Insert,
        Remove,
        Get,
        List,
    }
    impl PersistenceOperation {
        fn verb(&self) -> &'static str {
            match self {
                PersistenceOperation::Insert => "inserting",
                PersistenceOperation::Remove => "removing",
                PersistenceOperation::Get => "getting",
                PersistenceOperation::List => "listing",
            }
        }
    }

    pub type PersistenceResult<T> = Result<T, PersistenceError>;
    pub trait FlattenPersistenceResult<T>: Sized {
        fn flatten_persistence_result(self) -> PersistenceResult<T>;
    }
    impl<T> FlattenPersistenceResult<T> for PersistenceResult<PersistenceResult<T>> {
        fn flatten_persistence_result(self) -> PersistenceResult<T> {
            match self {
                Ok(Ok(ok)) => Ok(ok),
                Ok(Err(err)) => Err(err),
                Err(err) => Err(err)
            }
        }
    }
}
