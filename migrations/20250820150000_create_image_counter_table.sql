-- Create image_counter table for tracking image generation statistics
CREATE TABLE IF NOT EXISTS image_counter (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    image_type TEXT NOT NULL, -- Type of image (bn, song, leaderboard)
    count INTEGER NOT NULL DEFAULT 0,
    last_updated TEXT NOT NULL
);

-- Insert initial counter records if they don't exist
INSERT OR IGNORE INTO image_counter (image_type, count, last_updated) VALUES ('bn', 0, datetime('now'));
INSERT OR IGNORE INTO image_counter (image_type, count, last_updated) VALUES ('song', 0, datetime('now'));
INSERT OR IGNORE INTO image_counter (image_type, count, last_updated) VALUES ('leaderboard', 0, datetime('now'));

-- Create index for faster lookups
CREATE INDEX IF NOT EXISTS idx_image_counter_type ON image_counter (image_type);