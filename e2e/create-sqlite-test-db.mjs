import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import Database from 'better-sqlite3';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const fixturesDir = path.join(__dirname, 'fixtures');
const dbPath = path.join(fixturesDir, 'test.db');

fs.mkdirSync(fixturesDir, { recursive: true });
if (fs.existsSync(dbPath)) {
  fs.unlinkSync(dbPath);
}

const db = new Database(dbPath);
db.pragma('foreign_keys = ON');

db.exec(`
  CREATE TABLE users (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL,
    email TEXT UNIQUE,
    age INTEGER,
    created_at TEXT DEFAULT CURRENT_TIMESTAMP
  );

  CREATE TABLE posts (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id INTEGER REFERENCES users(id),
    title TEXT NOT NULL,
    content TEXT,
    published INTEGER DEFAULT 0,
    created_at TEXT DEFAULT CURRENT_TIMESTAMP
  );

  CREATE TABLE tags (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT UNIQUE NOT NULL
  );

  CREATE TABLE post_tags (
    post_id INTEGER REFERENCES posts(id),
    tag_id INTEGER REFERENCES tags(id),
    PRIMARY KEY (post_id, tag_id)
  );
`);

const insertUser = db.prepare(
  'INSERT INTO users (name, email, age) VALUES (?, ?, ?)',
);
const users = [
  ['Alice', 'alice@example.com', 25],
  ['Bob', 'bob@example.com', 32],
  ['Carol', 'carol@example.com', 19],
  ['Dave', 'dave@example.com', 41],
  ['Eve', 'eve@example.com', 22],
];
for (const u of users) {
  insertUser.run(...u);
}

const insertPost = db.prepare(
  'INSERT INTO posts (user_id, title, content, published) VALUES (?, ?, ?, ?)',
);
const posts = [
  [1, 'Hello SQLite', 'First post', 1],
  [1, 'Draft ideas', 'WIP', 0],
  [2, 'SQL tips', 'Use indexes', 1],
  [2, 'Private note', 'Secret', 0],
  [3, 'E2E tag test', 'Content', 1],
  [3, 'Another draft', 'N/A', 0],
  [4, 'Data modeling', 'FKs', 1],
  [4, 'Scratch', 'x', 0],
  [5, 'Views rock', 'Use CREATE VIEW', 1],
  [5, 'Unpublished', 'Should not show in view', 0],
];
for (const p of posts) {
  insertPost.run(...p);
}

const tagNames = ['sql', 'e2e', 'data', 'tip', 'blog'];
const insertTag = db.prepare('INSERT INTO tags (name) VALUES (?)');
for (const t of tagNames) {
  insertTag.run(t);
}

const postTagRows = [
  [1, 1],
  [1, 2],
  [3, 1],
  [3, 4],
  [5, 3],
  [7, 1],
  [7, 5],
  [9, 2],
  [9, 5],
  [1, 3],
];
const insertPostTag = db.prepare(
  'INSERT OR IGNORE INTO post_tags (post_id, tag_id) VALUES (?, ?)',
);
for (const [postId, tagId] of postTagRows) {
  insertPostTag.run(postId, tagId);
}

db.exec(`
  CREATE VIEW published_posts AS
  SELECT p.*, u.name AS author
  FROM posts p
  JOIN users u ON p.user_id = u.id
  WHERE p.published = 1
`);

db.close();
console.log('Wrote', dbPath);
