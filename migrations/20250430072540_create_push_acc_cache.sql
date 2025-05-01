-- Add migration script here

CREATE TABLE IF NOT EXISTS push_acc_cache (
    player_id TEXT NOT NULL,
    song_id TEXT NOT NULL,
    difficulty TEXT NOT NULL,
    last_checked_acc REAL NOT NULL, -- 上次检查该谱面时的 ACC
    last_check_time DATETIME NOT NULL, -- 上次检查时间
        PRIMARY KEY (player_id, song_id, difficulty)
);

CREATE INDEX IF NOT EXISTS idx_push_acc_cache_player_id ON push_acc_cache (player_id);