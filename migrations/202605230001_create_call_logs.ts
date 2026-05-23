import type { Kysely } from "kysely";

export async function up(db: Kysely<any>): Promise<void> {
  await db.schema
    .createTable("call_logs")
    .addColumn("id", "integer", (column) => column.primaryKey().autoIncrement())
    .addColumn("created_at", "text", (column) => column.notNull())
    .addColumn("duration_ms", "integer", (column) => column.notNull())
    .addColumn("status", "text", (column) => column.notNull())
    .addColumn("mode", "text", (column) => column.notNull())
    .addColumn("model", "text", (column) => column.notNull())
    .addColumn("tone", "text")
    .addColumn("input", "text", (column) => column.notNull())
    .addColumn("output_text", "text")
    .addColumn("error", "text")
    .addColumn("request_id", "text")
    .execute();

  await db.schema
    .createIndex("call_logs_created_at_id_idx")
    .on("call_logs")
    .columns(["created_at", "id"])
    .execute();
}

export async function down(db: Kysely<any>): Promise<void> {
  await db.schema.dropIndex("call_logs_created_at_id_idx").ifExists().execute();
  await db.schema.dropTable("call_logs").ifExists().execute();
}
