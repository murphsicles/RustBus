use sqlx::{Pool, Postgres};

pub async fn init_db(pool: &Pool<Postgres>, max_height: i64) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS transactions (
            txid TEXT NOT NULL,
            block_height BIGINT NOT NULL,
            tx_type TEXT NOT NULL,
            op_return TEXT,
            tx_hex TEXT NOT NULL,
            PRIMARY KEY (txid, block_height)
        ) PARTITION BY RANGE (block_height);
        CREATE TABLE IF NOT EXISTS blocks (
            block_hash TEXT NOT NULL,
            height BIGINT NOT NULL,
            prev_hash TEXT NOT NULL,
            timestamp BIGINT NOT NULL,
            PRIMARY KEY (block_hash, height)
        ) PARTITION BY RANGE (height);
        "#
    )
    .execute(pool)
    .await?;

    let partition_size = 100_000;
    for start in (0..=max_height).step_by(partition_size as usize) {
        let end = start + partition_size;
        sqlx::query(&format!(
            r#"
            CREATE TABLE IF NOT EXISTS transactions_{start}_{end} PARTITION OF transactions
                FOR VALUES FROM ({start}) TO ({end});
            CREATE TABLE IF NOT EXISTS blocks_{start}_{end} PARTITION OF blocks
                FOR VALUES FROM ({start}) TO ({end});
            CREATE INDEX IF NOT EXISTS idx_tx_type_{start}_{end} ON transactions_{start}_{end} (tx_type);
            CREATE INDEX IF NOT EXISTS idx_op_return_{start}_{end} ON transactions_{start}_{end} USING gin (op_return gin_trgm_ops);
            CREATE INDEX IF NOT EXISTS idx_block_height_{start}_{end} ON blocks_{start}_{end} (height);
            "#
        ))
        .execute(pool)
        .await?;
    }
    Ok(())
}
