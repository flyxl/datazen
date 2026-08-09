## Default Permission

Default permissions for the redis plugin (mutate + batch ops)

#### This default permission set includes the following:

- `allow-set-string`
- `allow-hash-set`
- `allow-hash-del`
- `allow-list-push`
- `allow-list-set`
- `allow-list-pop`
- `allow-set-add`
- `allow-set-remove`
- `allow-zset-add`
- `allow-zset-remove`
- `allow-delete-keys`
- `allow-rename`
- `allow-set-ttl`
- `allow-batch-delete-pattern`
- `allow-batch-set-ttl`
- `allow-batch-rename-prefix`
- `allow-flush-db`
- `allow-flush-all`
- `allow-count-matching`
- `allow-info`
- `allow-memory-sample`
- `allow-slowlog-get`
- `allow-slowlog-reset`
- `allow-modules-list`
- `allow-exec`
- `allow-pubsub-subscribe`
- `allow-pubsub-unsubscribe`
- `allow-pubsub-publish`
- `allow-json-get`
- `allow-json-set`
- `allow-json-del`
- `allow-xrange`
- `allow-xadd`
- `allow-xgroup-create`
- `allow-xgroup-destroy`
- `allow-xinfo-groups`
- `allow-xpending`
- `allow-xack`
- `allow-stream-overview`
- `allow-dump-keys`
- `allow-restore-keys`

## Permission Table

<table>
<tr>
<th>Identifier</th>
<th>Description</th>
</tr>


<tr>
<td>

`datazen-driver-redis:allow-batch-delete-pattern`

</td>
<td>

Enables the batch_delete_pattern command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`datazen-driver-redis:deny-batch-delete-pattern`

</td>
<td>

Denies the batch_delete_pattern command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`datazen-driver-redis:allow-batch-rename-prefix`

</td>
<td>

Enables the batch_rename_prefix command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`datazen-driver-redis:deny-batch-rename-prefix`

</td>
<td>

Denies the batch_rename_prefix command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`datazen-driver-redis:allow-batch-set-ttl`

</td>
<td>

Enables the batch_set_ttl command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`datazen-driver-redis:deny-batch-set-ttl`

</td>
<td>

Denies the batch_set_ttl command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`datazen-driver-redis:allow-count-matching`

</td>
<td>

Enables the count_matching command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`datazen-driver-redis:deny-count-matching`

</td>
<td>

Denies the count_matching command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`datazen-driver-redis:allow-delete-keys`

</td>
<td>

Enables the delete_keys command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`datazen-driver-redis:deny-delete-keys`

</td>
<td>

Denies the delete_keys command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`datazen-driver-redis:allow-dump-keys`

</td>
<td>

Enables the dump_keys command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`datazen-driver-redis:deny-dump-keys`

</td>
<td>

Denies the dump_keys command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`datazen-driver-redis:allow-exec`

</td>
<td>

Enables the exec command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`datazen-driver-redis:deny-exec`

</td>
<td>

Denies the exec command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`datazen-driver-redis:allow-flush-all`

</td>
<td>

Enables the flush_all command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`datazen-driver-redis:deny-flush-all`

</td>
<td>

Denies the flush_all command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`datazen-driver-redis:allow-flush-db`

</td>
<td>

Enables the flush_db command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`datazen-driver-redis:deny-flush-db`

</td>
<td>

Denies the flush_db command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`datazen-driver-redis:allow-hash-del`

</td>
<td>

Enables the hash_del command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`datazen-driver-redis:deny-hash-del`

</td>
<td>

Denies the hash_del command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`datazen-driver-redis:allow-hash-set`

</td>
<td>

Enables the hash_set command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`datazen-driver-redis:deny-hash-set`

</td>
<td>

Denies the hash_set command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`datazen-driver-redis:allow-info`

</td>
<td>

Enables the info command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`datazen-driver-redis:deny-info`

</td>
<td>

Denies the info command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`datazen-driver-redis:allow-json-del`

</td>
<td>

Enables the json_del command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`datazen-driver-redis:deny-json-del`

</td>
<td>

Denies the json_del command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`datazen-driver-redis:allow-json-get`

</td>
<td>

Enables the json_get command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`datazen-driver-redis:deny-json-get`

</td>
<td>

Denies the json_get command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`datazen-driver-redis:allow-json-set`

</td>
<td>

Enables the json_set command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`datazen-driver-redis:deny-json-set`

</td>
<td>

Denies the json_set command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`datazen-driver-redis:allow-list-pop`

</td>
<td>

Enables the list_pop command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`datazen-driver-redis:deny-list-pop`

</td>
<td>

Denies the list_pop command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`datazen-driver-redis:allow-list-push`

</td>
<td>

Enables the list_push command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`datazen-driver-redis:deny-list-push`

</td>
<td>

Denies the list_push command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`datazen-driver-redis:allow-list-set`

</td>
<td>

Enables the list_set command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`datazen-driver-redis:deny-list-set`

</td>
<td>

Denies the list_set command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`datazen-driver-redis:allow-memory-sample`

</td>
<td>

Enables the memory_sample command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`datazen-driver-redis:deny-memory-sample`

</td>
<td>

Denies the memory_sample command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`datazen-driver-redis:allow-modules-list`

</td>
<td>

Enables the modules_list command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`datazen-driver-redis:deny-modules-list`

</td>
<td>

Denies the modules_list command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`datazen-driver-redis:allow-pubsub-publish`

</td>
<td>

Enables the pubsub_publish command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`datazen-driver-redis:deny-pubsub-publish`

</td>
<td>

Denies the pubsub_publish command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`datazen-driver-redis:allow-pubsub-subscribe`

</td>
<td>

Enables the pubsub_subscribe command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`datazen-driver-redis:deny-pubsub-subscribe`

</td>
<td>

Denies the pubsub_subscribe command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`datazen-driver-redis:allow-pubsub-unsubscribe`

</td>
<td>

Enables the pubsub_unsubscribe command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`datazen-driver-redis:deny-pubsub-unsubscribe`

</td>
<td>

Denies the pubsub_unsubscribe command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`datazen-driver-redis:allow-rename`

</td>
<td>

Enables the rename command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`datazen-driver-redis:deny-rename`

</td>
<td>

Denies the rename command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`datazen-driver-redis:allow-restore-keys`

</td>
<td>

Enables the restore_keys command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`datazen-driver-redis:deny-restore-keys`

</td>
<td>

Denies the restore_keys command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`datazen-driver-redis:allow-set-add`

</td>
<td>

Enables the set_add command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`datazen-driver-redis:deny-set-add`

</td>
<td>

Denies the set_add command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`datazen-driver-redis:allow-set-remove`

</td>
<td>

Enables the set_remove command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`datazen-driver-redis:deny-set-remove`

</td>
<td>

Denies the set_remove command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`datazen-driver-redis:allow-set-string`

</td>
<td>

Enables the set_string command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`datazen-driver-redis:deny-set-string`

</td>
<td>

Denies the set_string command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`datazen-driver-redis:allow-set-ttl`

</td>
<td>

Enables the set_ttl command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`datazen-driver-redis:deny-set-ttl`

</td>
<td>

Denies the set_ttl command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`datazen-driver-redis:allow-slowlog-get`

</td>
<td>

Enables the slowlog_get command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`datazen-driver-redis:deny-slowlog-get`

</td>
<td>

Denies the slowlog_get command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`datazen-driver-redis:allow-slowlog-reset`

</td>
<td>

Enables the slowlog_reset command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`datazen-driver-redis:deny-slowlog-reset`

</td>
<td>

Denies the slowlog_reset command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`datazen-driver-redis:allow-stream-overview`

</td>
<td>

Enables the stream_overview command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`datazen-driver-redis:deny-stream-overview`

</td>
<td>

Denies the stream_overview command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`datazen-driver-redis:allow-xack`

</td>
<td>

Enables the xack command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`datazen-driver-redis:deny-xack`

</td>
<td>

Denies the xack command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`datazen-driver-redis:allow-xadd`

</td>
<td>

Enables the xadd command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`datazen-driver-redis:deny-xadd`

</td>
<td>

Denies the xadd command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`datazen-driver-redis:allow-xgroup-create`

</td>
<td>

Enables the xgroup_create command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`datazen-driver-redis:deny-xgroup-create`

</td>
<td>

Denies the xgroup_create command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`datazen-driver-redis:allow-xgroup-destroy`

</td>
<td>

Enables the xgroup_destroy command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`datazen-driver-redis:deny-xgroup-destroy`

</td>
<td>

Denies the xgroup_destroy command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`datazen-driver-redis:allow-xinfo-groups`

</td>
<td>

Enables the xinfo_groups command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`datazen-driver-redis:deny-xinfo-groups`

</td>
<td>

Denies the xinfo_groups command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`datazen-driver-redis:allow-xpending`

</td>
<td>

Enables the xpending command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`datazen-driver-redis:deny-xpending`

</td>
<td>

Denies the xpending command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`datazen-driver-redis:allow-xrange`

</td>
<td>

Enables the xrange command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`datazen-driver-redis:deny-xrange`

</td>
<td>

Denies the xrange command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`datazen-driver-redis:allow-zset-add`

</td>
<td>

Enables the zset_add command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`datazen-driver-redis:deny-zset-add`

</td>
<td>

Denies the zset_add command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`datazen-driver-redis:allow-zset-remove`

</td>
<td>

Enables the zset_remove command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`datazen-driver-redis:deny-zset-remove`

</td>
<td>

Denies the zset_remove command without any pre-configured scope.

</td>
</tr>
</table>
