//! Отвечает за управление данными.

use bb8::Pool;
use bb8_postgres::PostgresConnectionManager as PgConManager;
use core::marker::{Send, Sync};
use custom_error::custom_error;
use futures::future;
use tokio_postgres::{ToStatement, types::ToSql, row::Row, NoTls};

type MResult<T> = Result<T, Box<dyn std::error::Error>>;

custom_error!{NFO{} = "Не удалось получить данные."}
custom_error!{TNF{} = "Не удалось найти тег по идентификатору."}

/// Реализует операции ввода-вывода над пулом соединений с базой данных PostgreSQL.
#[derive(Clone)]
pub struct Db {
  pool: Pool<PgConManager<NoTls>>,
}

impl Db {
  /// Создаёт объект из пула соединений.
  pub fn new(pool: Pool<PgConManager<NoTls>>) -> Db {
    Db { pool }
  }

  /// Считывает одну строку из базы данных.
  pub async fn read<T>(&self, statement: &T, params: &[&(dyn ToSql + Sync)]) -> MResult<Row>
  where T: ?Sized + ToStatement {
    let cli = self.pool.get().await?;
    Ok(cli.query_one(statement, params).await?)
  }
  
  /// Записывает одно выражение в базу данных.
  pub async fn write<T>(&self, statement: &T, params: &[&(dyn ToSql + Sync)]) -> MResult<()>
  where T: ?Sized + ToStatement {
    let mut cli = self.pool.get().await?;
    let tr = cli.transaction().await?;
    tr.execute(statement, params).await?;
    tr.commit().await?;
    Ok(())
  }
  
  /// Считывает несколько значений по одной строке из базы данных.
  pub async fn read_mul<T>(&self, parts: Vec<(&T, Vec<&(dyn ToSql + Sync)>)>) -> MResult<Vec<Row>>
  where T: ?Sized + ToStatement + Send + Sync {
    let cli = self.pool.get().await?;
    let mut tasks = Vec::new();
    for i in 0..parts.len() {
      tasks.push(cli.query_one(parts[i].0, &parts[i].1));
    };
    let results = future::try_join_all(tasks).await?;
    Ok(results)
  }
  
  /// Записывает несколько значений в базу данных.
  pub async fn write_mul<T>(&self, parts: Vec<(&T, Vec<&(dyn ToSql + Sync)>)>) -> MResult<()>
  where T: ?Sized + ToStatement + Send + Sync {
    let mut cli = self.pool.get().await?;
    let tr = cli.transaction().await?;
    let mut tasks = Vec::new();
    for i in 0..parts.len() {
      tasks.push(tr.execute(parts[i].0, &parts[i].1));
    };
    future::try_join_all(tasks).await?;
    tr.commit().await?;
    Ok(())
  }
}
