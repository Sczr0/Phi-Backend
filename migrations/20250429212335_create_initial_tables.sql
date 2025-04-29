-- migrations/YYYYMMDDHHMMSS_create_initial_tables.sql (请使用你实际生成的文件名)

-- Create user_bindings table
CREATE TABLE user_bindings (
    qq TEXT PRIMARY KEY NOT NULL,
    session_token TEXT NOT NULL,
    nickname TEXT,
    last_update TEXT
);

-- Create unbind_verification_codes table
CREATE TABLE unbind_verification_codes (
    qq TEXT PRIMARY KEY NOT NULL,
    code TEXT NOT NULL,
    expires_at DATETIME NOT NULL
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
