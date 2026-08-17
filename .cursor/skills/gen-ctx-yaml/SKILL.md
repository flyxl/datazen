---
name: gen-ctx-yaml
description: >-
  Generate .ctx.yaml table group context files for NL2SQL optimization.
  Use when the user asks to create table groups, generate context files,
  or wants to organize database tables into named groups for AI-assisted
  SQL generation.
---

# Generate .ctx.yaml Table Group Files

Create `.ctx.yaml` files that define named table groups for NL2SQL context.
When a user `@`-references a `.ctx.yaml` file in the AI input, DataZen
automatically extracts the listed table names, fetches their real-time DDL,
and injects the schemas into the system prompt.

## Format

```yaml
groups:
  - name: "Group Name"
    tables:
      - table_name_1
      - schema.table_name_2
```

## Workflow

1. Ask the user which tables to include and how to group them.
   If the user provides a natural language description (e.g. "user management
   tables"), map it to concrete table names. If unsure, list candidates and
   confirm.

2. Generate the `.ctx.yaml` content following the format above.

3. Determine save location:
   - Default: the DataZen context directory (query via IPC `context_get_dir`,
     typically `{appData}/contexts/`).
   - The user may specify a custom path or filename.
   - Filename convention: `<descriptive-name>.ctx.yaml`
     (e.g. `user-management.ctx.yaml`, `sales-pipeline.ctx.yaml`).

4. Write the file and confirm to the user.

## Rules

- Each group must have a `name` (string) and `tables` (list of strings).
- Table names should match exactly what the database uses (case-sensitive).
- If the driver uses `schema.table` notation (e.g. PostgreSQL), include the
  schema prefix: `public.users`.
- Deduplicate tables across groups; a table may appear in multiple groups
  but avoid unnecessary repetition.
- Keep groups focused: 5–20 tables per group is ideal. For very large sets,
  split into multiple groups or multiple files.
- The file extension must be `.ctx.yaml` or `.ctx.yml`.

## Example

User: "Create a context file for our e-commerce database with user tables
and order tables"

Output (`ecommerce.ctx.yaml`):

```yaml
groups:
  - name: "User Management"
    tables:
      - users
      - user_roles
      - permissions
      - user_sessions
  - name: "Order Processing"
    tables:
      - orders
      - order_items
      - payments
      - shipping_addresses
```

## Integration

After creating the file, instruct the user:

> Place this file in your DataZen context directory. When writing SQL with
> AI assistance, type `@` and select the `.ctx.yaml` file. DataZen will
> automatically fetch the latest schemas for all listed tables and include
> them in the AI context.
