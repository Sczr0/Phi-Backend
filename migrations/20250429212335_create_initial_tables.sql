-- migrations/20250429212335_create_initial_tables.sql

-- Create internal_users table (新增)
CREATE TABLE internal_users (
    internal_id TEXT PRIMARY KEY NOT NULL,
    nickname TEXT,
    update_time TEXT NOT NULL
);

-- Create platform_bindings table (新增)
CREATE TABLE platform_bindings (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    internal_id TEXT NOT NULL,
    platform TEXT NOT NULL, -- 平台标识，如 "qq", "discord" 等
    platform_id TEXT NOT NULL, -- 平台用户ID
    session_token TEXT NOT NULL,
    bind_time TEXT NOT NULL,
    UNIQUE(platform, platform_id),
    FOREIGN KEY(internal_id) REFERENCES internal_users(internal_id)
);

-- Create player_archives table
CREATE TABLE player_archives (
    player_id TEXT PRIMARY KEY,
    player_name TEXT NOT NULL,
    rks REAL NOT NULL,
    update_time TEXT NOT NULL
);

-- Create chart_scores table
CREATE TABLE chart_scores (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    player_id TEXT NOT NULL,
    song_id TEXT NOT NULL,
    song_name TEXT NOT NULL,
    difficulty TEXT NOT NULL,
    difficulty_value REAL NOT NULL,
    score REAL NOT NULL,
    acc REAL NOT NULL,
    rks REAL NOT NULL,
    is_fc INTEGER NOT NULL,
    is_phi INTEGER NOT NULL,
    play_time TEXT NOT NULL,
    is_current INTEGER NOT NULL,
    UNIQUE(player_id, song_id, difficulty, play_time)
);

-- Create unbind_verification_codes table (修改)
CREATE TABLE unbind_verification_codes (
    platform TEXT NOT NULL,
    platform_id TEXT NOT NULL,
    code TEXT NOT NULL,
    expires_at DATETIME NOT NULL,
    PRIMARY KEY (platform, platform_id)
);

-- Create push_acc table
CREATE TABLE push_acc (
    player_id TEXT NOT NULL,
    song_id TEXT NOT NULL,
    difficulty TEXT NOT NULL,
    push_acc REAL NOT NULL,
    update_time TEXT NOT NULL,
    PRIMARY KEY (player_id, song_id, difficulty)
);

-- Create indexes
CREATE INDEX idx_chart_scores_player_current ON chart_scores (player_id, is_current);
CREATE INDEX idx_chart_scores_player_song_diff ON chart_scores (player_id, song_id, difficulty);
CREATE INDEX idx_platform_bindings_internal_id ON platform_bindings (internal_id);
CREATE INDEX idx_platform_bindings_platform ON platform_bindings (platform, platform_id);