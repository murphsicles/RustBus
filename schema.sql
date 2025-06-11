-- Parent table for transactions (partitioned)
CREATE TABLE transactions (
    txid TEXT NOT NULL,
    block_height BIGINT NOT NULL,
    tx_type TEXT NOT NULL,
    op_return TEXT,
    tx_hex TEXT NOT NULL,
    PRIMARY KEY (txid, block_height)
) PARTITION BY RANGE (block_height);

-- Parent table for blocks (partitioned)
CREATE TABLE blocks (
    block_hash TEXT NOT NULL,
    height BIGINT NOT NULL,
    prev_hash TEXT NOT NULL,
    PRIMARY KEY (block_hash, height)
) PARTITION BY RANGE (height);

-- Create partitions for initial ranges
CREATE TABLE transactions_0_100000 PARTITION OF transactions
    FOR VALUES FROM (0) TO (100000);
CREATE TABLE transactions_100000_200000 PARTITION OF transactions
    FOR VALUES FROM (100000) TO (200000);
CREATE TABLE blocks_0_100000 PARTITION OF blocks
    FOR VALUES FROM (0) TO (100000);
CREATE TABLE blocks_100000_200000 PARTITION OF blocks
    FOR VALUES FROM (100000) TO (200000);

-- Indexes on partitions
CREATE INDEX idx_tx_type_0_100000 ON transactions_0_100000 (tx_type);
CREATE INDEX idx_op_return_0_100000 ON transactions_0_100000 USING gin (op_return gin_trgm_ops);
CREATE INDEX idx_block_height_0_100000 ON blocks_0_100000 (height);
CREATE INDEX idx_tx_type_100000_200000 ON transactions_100000_200000 (tx_type);
CREATE INDEX idx_op_return_100000_200000 ON transactions_100000_200000 USING gin (op_return gin_trgm_ops);
CREATE INDEX idx_block_height_100000_200000 ON blocks_100000_200000 (height);
