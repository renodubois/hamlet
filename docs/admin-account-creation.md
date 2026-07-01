# Admin account creation

Server operators can create a password-backed Hamlet account from the command
line with the server admin CLI. The command writes directly to the configured
SQLite database and reuses the same password hashing and user creation path as
normal registration.

## Usage

From the server project during local development or source-based hosting:

```bash
cd server
cargo run --bin hamlet-admin -- create-user \
  --username alice \
  --password 'temporary-password'
```

For a built deployment, run the compiled `hamlet-admin` binary with the same
arguments:

```bash
hamlet-admin create-user --username alice --password 'temporary-password'
```

On success, the command prints the created username and user id. It does not
print the password.

## Target the same database as the server

Run the CLI with the same database environment used by the server process:

- `DATABASE_URL` overrides the database completely.
- `HAMLET_DATA_DIR` controls the default `hamlet.db` location when
  `DATABASE_URL` is not set.

Examples:

```bash
cd server
HAMLET_DATA_DIR=/var/lib/hamlet \
  cargo run --bin hamlet-admin -- create-user --username alice --password 'temporary-password'
```

```bash
cd server
DATABASE_URL='sqlite:///var/lib/hamlet/hamlet.db?mode=rwc' \
  cargo run --bin hamlet-admin -- create-user --username alice --password 'temporary-password'
```

For Docker Compose, run the command from an environment that can access the same
persistent SQLite volume or database URL as the server container.

## Error behavior

- Duplicate usernames fail with a clear `username "..." already exists` error.
- Missing flags, unknown flags, and blank username/password values fail before
  the database is modified and print command usage.
- Database setup or account creation failures exit non-zero for scripting.

The command creates the account only; it does not create a session, send an
invite, or force a password change. The invitee logs in through the normal client
with the temporary password.
