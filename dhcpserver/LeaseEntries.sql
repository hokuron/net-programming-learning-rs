CREATE TABLE IF NOT EXISTS lease_entries (
    "id"       INTEGER PRIMARY KEY AUTOINCREMENT,
    "mac_addr" TEXT NOT NULL UNIQUE,
    "ip_addr"  TEXT NOT NULL,
    "deleted"  unsigned INTEGER NOT NULL DEFAULT 0
);