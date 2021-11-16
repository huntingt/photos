CREATE TABLE users(
    user_id INTEGER NOT NULL,
    email STRING NOT NULL,
    password STRING NOT NULL,
    PRIMARY KEY (user_id)
);

CREATE TABLE sessions(
    key STRING NOT NULL,
    user_id INTEGER NOT NULL,
    PRIMARY KEY (key)
);

CREATE TABLE files(
    file_id STRING NOT NULL,
    owner_id INTEGER NOT NULL,
    width INTEGER NOT NULL,
    height INTEGER NOT NULL,
    last_modified INTEGER NOT NULL,
    name TEXT NOT NULL,
    mime TEXT NOT NULL,
    PRIMARY KEY (file_id)
);
