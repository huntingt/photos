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
