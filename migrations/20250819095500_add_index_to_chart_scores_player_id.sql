-- Add index on player_id for chart_scores table
CREATE INDEX idx_chart_scores_player_id ON chart_scores (player_id);