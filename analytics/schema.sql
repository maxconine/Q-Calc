-- one row per install: a random id the app made, nothing else about the person
CREATE TABLE IF NOT EXISTS installs (
  id TEXT PRIMARY KEY,
  platform TEXT NOT NULL,
  version TEXT NOT NULL,
  os TEXT,
  first_day TEXT NOT NULL,
  last_day TEXT NOT NULL
);

-- how many times an install did a thing on a day; the app resends a day until it is over, so n is replaced
CREATE TABLE IF NOT EXISTS usage (
  day TEXT NOT NULL,
  id TEXT NOT NULL,
  event TEXT NOT NULL,
  n INTEGER NOT NULL,
  PRIMARY KEY (day, id, event)
);
CREATE INDEX IF NOT EXISTS usage_day ON usage (day);

-- github's running download total per release asset, as seen each day
CREATE TABLE IF NOT EXISTS downloads (
  day TEXT NOT NULL,
  asset TEXT NOT NULL,
  platform TEXT NOT NULL,
  total INTEGER NOT NULL,
  PRIMARY KEY (day, asset)
);
