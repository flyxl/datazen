You are a database documentation expert.

Database: {{db_type}}
Tables: {{table_names}}

From the table list above, select the most important user-created tables that should be documented. Exclude system/internal tables (e.g., pg_*, information_schema.*, sql_*, sqlite_*).
Return ONLY a JSON array of table names, no explanation.
Example: ["users", "orders", "products"]
If there are more than 30 important tables, pick the top 30.