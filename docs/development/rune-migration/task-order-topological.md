# Rune 迁移：依赖拓扑排序与新任务划分（提案）

- **日期**：2026-11-19
- **源文档**：`docs/development/rune-migration/task-dependencies.md`（2722 条 task 级边）与 `tasklist.md`（621 个 remaining），`shards/segment-{1..4}.md` 仅用于读取归属。
- **方法**：解析 §2 的 `Tfrom->Tto`（含义：`from` 依赖 `to`，故 `to` 必须先于 `from`）。分层 Kahn 拓扑排序：维护 ready set（全部硬依赖已出队者），按 T 编号优先，每波取至多 4 个**互不相邻（全图无边）且 area 不同**的就绪任务，串联各波得到完整线性顺序。Tarjan 检测 SCC；对真实环丢弃环内“高 T→低 T”的边以打破环，环内按 T 编号线性化（迁移时“先接口后实现”解开）。
- **两种顺序**：(A) **Literal** — 保留全部 value 边、把 606 条 type-only/re-export 边降级为非硬约束；(B) **Rune-aware（主交付）** — 在 A 之上再把“目标为已满足基座”的边（指向 T091/T213/T341/T458）视为已满足并删除。两顺序都降级 type 边（不强制排序）。

## 0. 计数总览

| 指标 | 数值 |
| --- | ---: |
| remaining 任务节点 | 621 |
| 原始 task 级边 | 2722 |
| ├ value 边（真实运行时） | 2116 |
| └ type-only / re-export 边（已降级） | 606 |
| 反向边（低 T→高 T，原始 B） | 1406（value 1133） |
| 跨 shard 边 | 1602 |
| Literal 硬约束边数 | 2116 |
| Rune-aware 硬约束边数 | 1814 |
| 被删除的“已满足基座”边（全部 / value） | 538 / 302（type 236） |
| Literal SCC 数（多节点） | 7 |
| Rune-aware SCC 数（多节点） | 7 |
| Literal wave 数 | 203 |
| Rune-aware wave 数 | 203 |

## 1. Rune-aware 拓扑顺序（主交付 · 新执行顺序）

以下为本提案建议的执行顺序：按依赖拓扑排列，wave 内仅并行互不依赖、跨 area 的任务。每段标注 T id / area / 当前 shard，并给出跨 shard 协调警告。

### Wave 1 — T014, T029, T033, T038
- **任务/area**：T014(components/export), T029(lib/connectionViews), T033(lib/relationPrediction), T038(hooks)
- **当前 shard**：T014:seg2, T029:seg4, T033:seg2, T038:seg4
- **跨 shard 警告**：wave 内混段（seg2/seg4）；跨段依赖：T014→[T341:seg4]; T029→[T213:seg2]

### Wave 2 — T042, T046, T049, T050
- **任务/area**：T042(types), T046(extensions), T049(ai), T050(app.ts)
- **当前 shard**：T042:seg2, T046:seg2, T049:seg1, T050:seg4
- **跨 shard 警告**：wave 内混段（seg1/seg2/seg4）

### Wave 3 — T053, T075, T077, T079
- **任务/area**：T053(shell.ts), T075(windows/data-sync), T077(windows/backup), T079(lib/chart)
- **当前 shard**：T053:seg1, T075:seg3, T077:seg3, T079:seg3
- **跨 shard 警告**：wave 内混段（seg1/seg3）；跨段依赖：T053→[T341:seg4, T386:seg4, T473:seg3, T623:seg2]; T075→[T341:seg4]; T077→[T341:seg4]; T079→[T091:seg2, T213:seg2]

### Wave 4 — T080, T081, T091, T093
- **任务/area**：T080(lib/metadata), T081(lib/relationMetadata), T091(types), T093(platform/shared)
- **当前 shard**：T080:seg2, T081:seg1, T091:seg2, T093:seg4
- **跨 shard 警告**：wave 内混段（seg1/seg2/seg4）；跨段依赖：T081→[T121:seg2, T213:seg2, T579:seg2]

### Wave 5 — T031, T044, T095, T101
- **任务/area**：T031(lib/metadata), T044(platform/shared), T095(extensions), T101(components/history)
- **当前 shard**：T031:seg2, T044:seg4, T095:seg2, T101:seg4
- **跨 shard 警告**：wave 内混段（seg2/seg4）；跨段依赖：T101→[T213:seg2]

### Wave 6 — T105, T119, T120, T121
- **任务/area**：T105(components/ai), T119(lib/chart), T120(lib/metadata), T121(lib/relationPrediction)
- **当前 shard**：T105:seg3, T119:seg3, T120:seg2, T121:seg2
- **跨 shard 警告**：wave 内混段（seg2/seg3）；跨段依赖：T105→[T213:seg2, T341:seg4]; T119→[T213:seg2]

### Wave 7 — T082, T122, T123, T128
- **任务/area**：T082(lib/relationPrediction), T122(lib/session), T123(lib/sqlDialects), T128(stores/ai)
- **当前 shard**：T082:seg2, T122:seg1, T123:seg4, T128:seg2
- **跨 shard 警告**：wave 内混段（seg1/seg2/seg4）；跨段依赖：T128→[T164:seg1]

### Wave 8 — T130, T132, T133, T135
- **任务/area**：T130(types), T132(commands), T133(ai), T135(components/filter)
- **当前 shard**：T130:seg2, T132:seg4, T133:seg1, T135:seg1
- **跨 shard 警告**：wave 内混段（seg1/seg2/seg4）；跨段依赖：T135→[T341:seg4]

### Wave 9 — T058, T138, T142, T150
- **任务/area**：T058(components/query), T138(components/chart), T142(components), T150(windows/data-sync)
- **当前 shard**：T058:seg3, T138:seg2, T142:seg3, T150:seg3
- **跨 shard 警告**：wave 内混段（seg2/seg3）；跨段依赖：T058→[T132:seg4, T341:seg4]; T138→[T079:seg3]; T150→[T341:seg4, T406:seg4]

### Wave 10 — T136, T151, T154, T156
- **任务/area**：T136(components/query), T151(windows/schema-diff), T154(lib/session), T156(lib/structureEditor)
- **当前 shard**：T136:seg3, T151:seg4, T154:seg1, T156:seg1
- **跨 shard 警告**：wave 内混段（seg1/seg3/seg4）；跨段依赖：T136→[T341:seg4, T577:seg2]

### Wave 11 — T083, T085, T159, T160
- **任务/area**：T083(lib/session), T085(lib/structureEditor), T159(stores), T160(stores/tableData)
- **当前 shard**：T083:seg1, T085:seg1, T159:seg3, T160:seg3
- **跨 shard 警告**：wave 内混段（seg1/seg3）；跨段依赖：T160→[T213:seg2, T629:seg2]

### Wave 12 — T161, T162, T163, T164
- **任务/area**：T161(types), T162(platform), T163(commands), T164(ai)
- **当前 shard**：T161:seg2, T162:seg3, T163:seg4, T164:seg1
- **跨 shard 警告**：wave 内混段（seg1/seg2/seg3/seg4）

### Wave 13 — T166, T176, T185, T188
- **任务/area**：T166(components/filter), T176(windows/workflow), T185(hooks), T188(types)
- **当前 shard**：T166:seg1, T176:seg2, T185:seg4, T188:seg2
- **跨 shard 警告**：wave 内混段（seg1/seg2/seg4）

### Wave 14 — T189, T204, T209, T211
- **任务/area**：T189(platform), T204(windows/data-transfer), T209(lib/valuePresentation), T211(stores)
- **当前 shard**：T189:seg3, T204:seg3, T209:seg4, T211:seg3
- **跨 shard 警告**：wave 内混段（seg3/seg4）；跨段依赖：T189→[T298:seg4]; T204→[T213:seg2, T341:seg4, T406:seg4]

### Wave 15 — T212, T214, T227, T229
- **任务/area**：T212(stores/tableData), T214(commands), T227(windows/data-transfer), T229(windows/schema-diff)
- **当前 shard**：T212:seg3, T214:seg4, T227:seg3, T229:seg4
- **跨 shard 警告**：wave 内混段（seg3/seg4）；跨段依赖：T214→[T213:seg2]; T227→[T341:seg4, T415:seg4]

### Wave 16 — T233, T236, T237, T242
- **任务/area**：T233(hooks), T236(types), T237(commands), T242(components/sql-editor)
- **当前 shard**：T233:seg4, T236:seg2, T237:seg4, T242:seg4
- **跨 shard 警告**：wave 内混段（seg2/seg4）；跨段依赖：T242→[T081:seg1]

### Wave 17 — T063, T146, T246, T251
- **任务/area**：T063(components/sql-editor), T146(windows/dashboard), T246(windows/workflow), T251(lib)
- **当前 shard**：T063:seg4, T146:seg4, T246:seg2, T251:seg2
- **跨 shard 警告**：wave 内混段（seg2/seg4）；跨段依赖：T063→[T213:seg2]; T146→[T161:seg2]

### Wave 18 — T213, T267, T268, T271
- **任务/area**：T213(types), T267(windows/workflow), T268(windows/onboarding), T271(windows/schema-diff)
- **当前 shard**：T213:seg2, T267:seg2, T268:seg4, T271:seg4
- **跨 shard 警告**：wave 内混段（seg2/seg4）；跨段依赖：T213→[T184:seg4]

### Wave 19 — T202, T257, T272, T273
- **任务/area**：T202(windows/workflow), T257(types), T272(lib), T273(lib/session)
- **当前 shard**：T202:seg2, T257:seg2, T272:seg2, T273:seg1
- **跨 shard 警告**：wave 内混段（seg1/seg2）

### Wave 20 — T274, T276, T278, T279
- **任务/area**：T274(lib/valuePresentation), T276(stores), T278(types), T279(commands)
- **当前 shard**：T274:seg4, T276:seg3, T278:seg2, T279:seg4
- **跨 shard 警告**：wave 内混段（seg2/seg3/seg4）

### Wave 21 — T048, T183, T232, T254
- **任务/area**：T048(commands), T183(lib/session), T232(lib/valuePresentation), T254(hooks)
- **当前 shard**：T048:seg4, T183:seg1, T232:seg4, T254:seg4
- **跨 shard 警告**：wave 内混段（seg1/seg4）；跨段依赖：T183→[T276:seg3]; T254→[T213:seg2]

### Wave 22 — T208, T253, T283, T287
- **任务/area**：T208(lib/session), T253(lib/valuePresentation), T283(components/sql-editor), T287(windows/onboarding)
- **当前 shard**：T208:seg1, T253:seg4, T283:seg4, T287:seg4
- **跨 shard 警告**：wave 内混段（seg1/seg4）

### Wave 23 — T231, T288, T293, T295
- **任务/area**：T231(lib/session), T288(windows/data-transfer), T293(lib/valuePresentation), T295(stores)
- **当前 shard**：T231:seg1, T288:seg3, T293:seg4, T295:seg3
- **跨 shard 警告**：wave 内混段（seg1/seg3/seg4）；跨段依赖：T231→[T213:seg2]; T288→[T341:seg4]

### Wave 24 — T157, T249, T252, T297
- **任务/area**：T157(lib/valuePresentation), T249(windows/data-sync), T252(lib/session), T297(types)
- **当前 shard**：T157:seg4, T249:seg3, T252:seg1, T297:seg2
- **跨 shard 警告**：wave 内混段（seg1/seg2/seg3/seg4）；跨段依赖：T249→[T341:seg4, T406:seg4]; T252→[T213:seg2, T458:seg3]

### Wave 25 — T086, T292, T298, T303
- **任务/area**：T086(lib/valuePresentation), T292(lib/session), T298(commands), T303(components)
- **当前 shard**：T086:seg4, T292:seg1, T298:seg4, T303:seg3
- **跨 shard 警告**：wave 内混段（seg1/seg3/seg4）；跨段依赖：T292→[T629:seg2]; T298→[T131:seg3, T213:seg2]

### Wave 26 — T131, T184, T307, T310
- **任务/area**：T131(platform), T184(lib/valuePresentation), T307(windows/data-transfer), T310(lib)
- **当前 shard**：T131:seg3, T184:seg4, T307:seg3, T310:seg2
- **跨 shard 警告**：wave 内混段（seg2/seg3/seg4）；跨段依赖：T131→[T213:seg2, T298:seg4, T484:seg4]; T307→[T415:seg4]

### Wave 27 — T021, T178, T311, T312
- **任务/area**：T021(windows/onboarding), T178(windows/data-transfer), T311(lib/session), T312(lib/valuePresentation)
- **当前 shard**：T021:seg4, T178:seg3, T311:seg1, T312:seg4
- **跨 shard 警告**：wave 内混段（seg1/seg3/seg4）；跨段依赖：T021→[T131:seg3]; T178→[T341:seg4, T415:seg4]; T311→[T039:seg3, T213:seg2, T276:seg3]

### Wave 28 — T125, T315, T316, T317
- **任务/area**：T125(lib/valuePresentation), T315(stores/tableData), T316(commands), T317(components/DataTable)
- **当前 shard**：T125:seg4, T315:seg3, T316:seg4, T317:seg3
- **跨 shard 警告**：wave 内混段（seg3/seg4）；跨段依赖：T317→[T341:seg4]

### Wave 29 — T037, T055, T140, T158
- **任务/area**：T037(lib/valuePresentation), T055(components/DataTable), T140(components/migration), T158(hooks)
- **当前 shard**：T037:seg4, T055:seg3, T140:seg1, T158:seg4
- **跨 shard 警告**：wave 内混段（seg1/seg3/seg4）；跨段依赖：T055→[T125:seg4, T157:seg4, T184:seg4, T341:seg4]; T140→[T316:seg4, T341:seg4]

### Wave 30 — T323, T324, T327, T331
- **任务/area**：T323(windows/onboarding), T324(windows/data-sync), T327(lib/session), T331(commands)
- **当前 shard**：T323:seg4, T324:seg3, T327:seg1, T331:seg4
- **跨 shard 警告**：wave 内混段（seg1/seg3/seg4）；跨段依赖：T324→[T341:seg4, T625:seg2]; T327→[T213:seg2]

### Wave 31 — T116, T332, T335, T339
- **任务/area**：T116(windows/data-sync), T332(components/DataTable), T335(components/sql-editor), T339(windows/schema-diff)
- **当前 shard**：T116:seg3, T332:seg3, T335:seg4, T339:seg4
- **跨 shard 警告**：wave 内混段（seg3/seg4）；跨段依赖：T116→[T341:seg4, T406:seg4]

### Wave 32 — T196, T341, T356, T363
- **任务/area**：T196(components/sql-editor), T341(hooks), T356(commands), T363(windows/schema-diff)
- **当前 shard**：T196:seg4, T341:seg4, T356:seg4, T363:seg4
- **跨 shard 警告**：跨段依赖：T356→[T213:seg2]

### Wave 33 — T290, T350, T353, T366
- **任务/area**：T290(windows/schema-diff), T350(windows/data-sync), T353(hooks), T366(stores)
- **当前 shard**：T290:seg4, T350:seg3, T353:seg4, T366:seg3
- **跨 shard 警告**：wave 内混段（seg3/seg4）；跨段依赖：T350→[T341:seg4, T406:seg4]; T366→[T213:seg2, T601:seg2]

### Wave 34 — T205, T250, T365, T367
- **任务/area**：T205(windows/data-sync), T250(windows/schema-diff), T365(hooks), T367(stores/tableData)
- **当前 shard**：T205:seg3, T250:seg4, T365:seg4, T367:seg3
- **跨 shard 警告**：wave 内混段（seg3/seg4）；跨段依赖：T205→[T341:seg4, T406:seg4]; T365→[T458:seg3]; T367→[T213:seg2, T629:seg2]

### Wave 35 — T018, T368, T370, T374
- **任务/area**：T018(windows/dashboard), T368(commands), T370(components/sql-editor), T374(windows/schema-diff)
- **当前 shard**：T018:seg4, T368:seg4, T370:seg4, T374:seg4
- **跨 shard 警告**：跨段依赖：T018→[T161:seg2]; T374→[T213:seg2]

### Wave 36 — T379, T381, T385, T386
- **任务/area**：T379(commands), T381(windows/settings), T385(lib), T386(hooks)
- **当前 shard**：T379:seg4, T381:seg4, T385:seg2, T386:seg4
- **跨 shard 警告**：wave 内混段（seg2/seg4）；跨段依赖：T379→[T213:seg2]; T386→[T131:seg3]

### Wave 37 — T180, T387, T388, T399
- **任务/area**：T180(windows/schema-diff), T387(stores), T388(commands), T399(windows/settings)
- **当前 shard**：T180:seg4, T387:seg3, T388:seg4, T399:seg4
- **跨 shard 警告**：wave 内混段（seg3/seg4）；跨段依赖：T388→[T131:seg3, T213:seg2]

### Wave 38 — T109, T393, T396, T397
- **任务/area**：T109(windows/settings), T393(windows/schema-diff), T396(stores), T397(commands)
- **当前 shard**：T109:seg4, T393:seg4, T396:seg3, T397:seg4
- **跨 shard 警告**：wave 内混段（seg3/seg4）；跨段依赖：T109→[T213:seg2]; T397→[T156:seg1, T311:seg1]

### Wave 39 — T071, T244, T402, T406
- **任务/area**：T071(windows/main), T244(windows/settings), T402(windows/schema-diff), T406(commands)
- **当前 shard**：T071:seg2, T244:seg4, T402:seg4, T406:seg4
- **跨 shard 警告**：wave 内混段（seg2/seg4）；跨段依赖：T071→[T038:seg4, T396:seg3]

### Wave 40 — T304, T362, T407, T414
- **任务/area**：T304(windows/settings), T362(windows/data-sync), T407(components/DataTable), T414(stores)
- **当前 shard**：T304:seg4, T362:seg3, T407:seg3, T414:seg3
- **跨 shard 警告**：wave 内混段（seg3/seg4）；跨段依赖：T362→[T341:seg4, T406:seg4]; T407→[T213:seg2]

### Wave 41 — T265, T410, T415, T416
- **任务/area**：T265(windows/settings), T410(windows/data-sync), T415(commands), T416(components/DataTable)
- **当前 shard**：T265:seg4, T410:seg3, T415:seg4, T416:seg3
- **跨 shard 警告**：wave 内混段（seg3/seg4）；跨段依赖：T265→[T095:seg2, T213:seg2, T458:seg3]; T415→[T213:seg2]; T416→[T257:seg2, T523:seg2]

### Wave 42 — T023, T345, T417, T419
- **任务/area**：T023(windows/data-transfer), T345(components/DataTable), T417(windows/settings), T419(windows/data-sync)
- **当前 shard**：T023:seg3, T345:seg3, T417:seg4, T419:seg3
- **跨 shard 警告**：wave 内混段（seg3/seg4）；跨段依赖：T023→[T341:seg4, T415:seg4]; T345→[T157:seg4, T213:seg2]; T419→[T213:seg2, T406:seg4]

### Wave 43 — T269, T308, T423, T424
- **任务/area**：T269(windows/data-transfer), T308(windows/data-sync), T423(stores), T424(commands)
- **当前 shard**：T269:seg3, T308:seg3, T423:seg3, T424:seg4
- **跨 shard 警告**：wave 内混段（seg3/seg4）；跨段依赖：T269→[T341:seg4, T415:seg4]; T308→[T341:seg4, T406:seg4]

### Wave 44 — T338, T427, T429, T432
- **任务/area**：T338(windows/data-sync), T427(windows/connection), T429(lib), T432(commands)
- **当前 shard**：T338:seg3, T427:seg1, T429:seg2, T432:seg4
- **跨 shard 警告**：wave 内混段（seg1/seg2/seg3/seg4）；跨段依赖：T338→[T341:seg4]; T427→[T121:seg2, T213:seg2, T341:seg4]; T432→[T213:seg2]

### Wave 45 — T179, T437, T440, T441
- **任务/area**：T179(windows/data-sync), T437(lib), T440(commands), T441(windows/settings)
- **当前 shard**：T179:seg3, T437:seg2, T440:seg4, T441:seg4
- **跨 shard 警告**：wave 内混段（seg2/seg3/seg4）；跨段依赖：T179→[T341:seg4, T406:seg4]; T437→[T131:seg3]; T440→[T297:seg2]; T441→[T213:seg2, T636:seg2]

### Wave 46 — T186, T383, T413, T444
- **任务/area**：T186(stores), T383(windows/data-sync), T413(hooks), T444(lib)
- **当前 shard**：T186:seg3, T383:seg3, T413:seg4, T444:seg2
- **跨 shard 警告**：wave 内混段（seg2/seg3/seg4）；跨段依赖：T186→[T161:seg2, T237:seg4, T437:seg2]; T383→[T341:seg4]; T413→[T437:seg2, T458:seg3]

### Wave 47 — T024, T445, T450, T452
- **任务/area**：T024(windows/data-sync), T445(hooks), T450(lib), T452(stores)
- **当前 shard**：T024:seg3, T445:seg4, T450:seg2, T452:seg3
- **跨 shard 警告**：wave 内混段（seg2/seg3/seg4）；跨段依赖：T024→[T341:seg4, T406:seg4]; T452→[T617:seg2]

### Wave 48 — T035, T107, T124, T182
- **任务/area**：T035(lib/sqlDialects), T107(components/sql-editor), T124(lib/structureEditor), T182(lib/chart)
- **当前 shard**：T035:seg4, T107:seg4, T124:seg1, T182:seg3
- **跨 shard 警告**：wave 内混段（seg1/seg3/seg4）；跨段依赖：T035→[T213:seg2, T450:seg2]; T107→[T213:seg2, T450:seg2, T617:seg2]; T124→[T213:seg2, T450:seg2]; T182→[T091:seg2, T213:seg2, T450:seg2]

### Wave 49 — T030, T036, T210, T364
- **任务/area**：T030(lib/dashboard), T036(lib/structureEditor), T210(hooks), T364(lib)
- **当前 shard**：T030:seg3, T036:seg1, T210:seg4, T364:seg2
- **跨 shard 警告**：wave 内混段（seg1/seg2/seg3/seg4）；跨段依赖：T030→[T091:seg2, T161:seg2, T213:seg2]; T210→[T182:seg3, T213:seg2]; T364→[T333:seg1]

### Wave 50 — T065, T318, T392, T394
- **任务/area**：T065(components/admin), T318(components/connection), T392(windows/data-sync), T394(lib)
- **当前 shard**：T065:seg3, T318:seg1, T392:seg3, T394:seg2
- **跨 shard 警告**：wave 内混段（seg1/seg2/seg3）；跨段依赖：T065→[T213:seg2, T364:seg2]; T318→[T213:seg2, T364:seg2]; T392→[T406:seg4]

### Wave 51 — T373, T375, T451, T459
- **任务/area**：T373(windows/data-sync), T375(lib), T451(hooks), T459(windows/settings)
- **当前 shard**：T373:seg3, T375:seg2, T451:seg4, T459:seg4
- **跨 shard 警告**：wave 内混段（seg2/seg3/seg4）；跨段依赖：T373→[T341:seg4, T406:seg4]; T451→[T458:seg3]; T459→[T213:seg2]

### Wave 52 — T190, T228, T457, T465
- **任务/area**：T190(commands), T228(windows/data-sync), T457(hooks), T465(windows/connection)
- **当前 shard**：T190:seg4, T228:seg3, T457:seg4, T465:seg1
- **跨 shard 警告**：wave 内混段（seg1/seg3/seg4）；跨段依赖：T190→[T213:seg2, T375:seg2]; T228→[T341:seg4, T406:seg4]

### Wave 53 — T306, T401, T468, T469
- **任务/area**：T306(windows/onboarding), T401(windows/data-sync), T468(stores), T469(windows/settings)
- **当前 shard**：T306:seg4, T401:seg3, T468:seg3, T469:seg4
- **跨 shard 警告**：wave 内混段（seg3/seg4）；跨段依赖：T306→[T137:seg1]; T401→[T406:seg4]; T468→[T213:seg2, T432:seg4]

### Wave 54 — T358, T428, T472, T473
- **任务/area**：T358(components/connection), T428(windows/data-sync), T472(lib), T473(stores)
- **当前 shard**：T358:seg1, T428:seg3, T472:seg2, T473:seg3
- **跨 shard 警告**：wave 内混段（seg1/seg2/seg3）；跨段依赖：T358→[T213:seg2, T341:seg4, T432:seg4, T468:seg3]; T428→[T406:seg4]

### Wave 55 — T461, T474, T475, T477
- **任务/area**：T461(windows/data-sync), T474(windows/settings), T475(windows/connection), T477(lib)
- **当前 shard**：T461:seg3, T474:seg4, T475:seg1, T477:seg2
- **跨 shard 警告**：wave 内混段（seg1/seg2/seg3/seg4）；跨段依赖：T474→[T458:seg3]; T475→[T039:seg3]; T477→[T039:seg3]

### Wave 56 — T434, T466, T482, T488
- **任务/area**：T434(windows/settings), T466(windows/data-sync), T482(lib), T488(stores)
- **当前 shard**：T434:seg4, T466:seg3, T482:seg2, T488:seg3
- **跨 shard 警告**：wave 内混段（seg2/seg3/seg4）；跨段依赖：T434→[T257:seg2, T458:seg3]; T466→[T341:seg4, T406:seg4]; T488→[T161:seg2, T213:seg2, T225:seg2]

### Wave 57 — T471, T484, T491, T492
- **任务/area**：T471(windows/data-sync), T484(windows/settings), T491(lib), T492(windows/connection)
- **当前 shard**：T471:seg3, T484:seg4, T491:seg2, T492:seg1
- **跨 shard 警告**：wave 内混段（seg1/seg2/seg3/seg4）；跨段依赖：T471→[T406:seg4]; T484→[T070:seg2, T213:seg2, T297:seg2]; T492→[T341:seg4, T629:seg2]

### Wave 58 — T013, T092, T096, T255
- **任务/area**：T013(components/admin), T092(platform), T096(commands), T255(stores)
- **当前 shard**：T013:seg3, T092:seg3, T096:seg4, T255:seg3
- **跨 shard 警告**：wave 内混段（seg3/seg4）；跨段依赖：T013→[T254:seg4, T279:seg4, T341:seg4, T491:seg2]; T092→[T484:seg4]; T096→[T131:seg3, T164:seg1, T213:seg2]; T255→[T331:seg4, T491:seg2]

### Wave 59 — T059, T087, T097, T207
- **任务/area**：T059(components/connection), T087(hooks), T097(ai), T207(lib)
- **当前 shard**：T059:seg1, T087:seg4, T097:seg1, T207:seg2
- **跨 shard 警告**：wave 内混段（seg1/seg2/seg4）；跨段依赖：T059→[T096:seg4, T341:seg4]; T087→[T049:seg1, T164:seg1, T208:seg1]; T097→[T096:seg4]; T207→[T096:seg4, T356:seg4]

### Wave 60 — T266, T285, T455, T483
- **任务/area**：T266(windows/connection), T285(windows/settings), T455(windows/data-sync), T483(stores)
- **当前 shard**：T266:seg1, T285:seg4, T455:seg3, T483:seg3
- **跨 shard 警告**：wave 内混段（seg1/seg3/seg4）；跨段依赖：T266→[T013:seg3, T341:seg4]; T285→[T255:seg3]; T455→[T096:seg4, T341:seg4]; T483→[T297:seg2, T440:seg4, T491:seg2]

### Wave 61 — T022, T198, T224, T481
- **任务/area**：T022(windows/wapps), T198(windows/settings), T224(windows/workspace), T481(windows/data-sync)
- **当前 shard**：T022:seg2, T198:seg4, T224:seg4, T481:seg3
- **跨 shard 警告**：wave 内混段（seg2/seg3/seg4）；跨段依赖：T022→[T341:seg4, T440:seg4, T483:seg3, T484:seg4]; T198→[T213:seg2, T458:seg3, T483:seg3, T491:seg2]; T224→[T297:seg2, T483:seg3, T488:seg3]; T481→[T406:seg4]

### Wave 62 — T073, T371, T486, T494
- **任务/area**：T073(windows/wapps), T371(windows/settings), T486(windows/data-sync), T494(lib)
- **当前 shard**：T073:seg2, T371:seg4, T486:seg3, T494:seg2
- **跨 shard 警告**：wave 内混段（seg2/seg3/seg4）；跨段依赖：T073→[T038:seg4, T224:seg4, T341:seg4, T440:seg4, T483:seg3, T484:seg4, T488:seg3]; T371→[T213:seg2, T450:seg2]; T486→[T406:seg4]

### Wave 63 — T493, T497, T501
- **任务/area**：T493(windows/data-sync), T497(lib), T501(windows/connection)
- **当前 shard**：T493:seg3, T497:seg2, T501:seg1
- **跨 shard 警告**：wave 内混段（seg1/seg2/seg3）；跨段依赖：T493→[T406:seg4]; T501→[T341:seg4]

### Wave 64 — T499, T505, T530
- **任务/area**：T499(windows/data-sync), T505(lib), T530(windows/connection)
- **当前 shard**：T499:seg3, T505:seg2, T530:seg1
- **跨 shard 警告**：wave 内混段（seg1/seg2/seg3）；跨段依赖：T499→[T406:seg4]; T530→[T341:seg4]

### Wave 65 — T289, T509, T532
- **任务/area**：T289(windows/data-sync), T509(lib), T532(windows/connection)
- **当前 shard**：T289:seg3, T509:seg2, T532:seg1
- **跨 shard 警告**：wave 内混段（seg1/seg2/seg3）；跨段依赖：T289→[T341:seg4]; T532→[T341:seg4]

### Wave 66 — T496, T511, T538
- **任务/area**：T496(windows/data-sync), T511(lib), T538(windows/connection)
- **当前 shard**：T496:seg3, T511:seg2, T538:seg1
- **跨 shard 警告**：wave 内混段（seg1/seg2/seg3）；跨段依赖：T538→[T213:seg2, T341:seg4]

### Wave 67 — T502, T513, T542
- **任务/area**：T502(windows/data-sync), T513(lib), T542(windows/connection)
- **当前 shard**：T502:seg3, T513:seg2, T542:seg1
- **跨 shard 警告**：wave 内混段（seg1/seg2/seg3）；跨段依赖：T502→[T213:seg2]; T542→[T341:seg4, T366:seg3, T515:seg2]

### Wave 68 — T187, T216, T436, T515
- **任务/area**：T187(stores/tableData), T216(components/filter), T436(windows/data-sync), T515(lib)
- **当前 shard**：T187:seg3, T216:seg1, T436:seg3, T515:seg2
- **跨 shard 警告**：wave 内混段（seg1/seg2/seg3）；跨段依赖：T187→[T213:seg2, T513:seg2]; T216→[T213:seg2, T513:seg2]; T436→[T341:seg4, T406:seg4]

### Wave 69 — T056, T234, T490, T521
- **任务/area**：T056(components/filter), T234(stores), T490(windows/data-sync), T521(lib)
- **当前 shard**：T056:seg1, T234:seg3, T490:seg3, T521:seg2
- **跨 shard 警告**：wave 内混段（seg1/seg2/seg3）；跨段依赖：T056→[T187:seg3, T213:seg2, T513:seg2]; T234→[T515:seg2]; T490→[T406:seg4]

### Wave 70 — T108, T523, T544
- **任务/area**：T108(components), T523(lib), T544(windows/connection)
- **当前 shard**：T108:seg3, T523:seg2, T544:seg1
- **跨 shard 警告**：wave 内混段（seg1/seg2/seg3）；跨段依赖：T108→[T056:seg1, T166:seg1, T213:seg2, T216:seg1, T341:seg4, T513:seg2]; T544→[T341:seg4, T525:seg2]

### Wave 71 — T500, T548
- **任务/area**：T500(lib), T548(windows/connection)
- **当前 shard**：T500:seg2, T548:seg1
- **跨 shard 警告**：wave 内混段（seg1/seg2）；跨段依赖：T500→[T189:seg3]; T548→[T341:seg4]

### Wave 72 — T043, T525, T564
- **任务/area**：T043(platform), T525(lib), T564(windows/connection)
- **当前 shard**：T043:seg3, T525:seg2, T564:seg1
- **跨 shard 警告**：wave 内混段（seg1/seg2/seg3）；跨段依赖：T043→[T500:seg2, T523:seg2]

### Wave 73 — T094, T275, T425, T456
- **任务/area**：T094(platform/web), T275(hooks), T425(components/DataTable), T456(lib)
- **当前 shard**：T094:seg1, T275:seg4, T425:seg3, T456:seg2
- **跨 shard 警告**：wave 内混段（seg1/seg2/seg3/seg4）；跨段依赖：T094→[T043:seg3, T092:seg3, T131:seg3, T437:seg2]; T275→[T525:seg2]; T425→[T525:seg2]; T456→[T043:seg3, T131:seg3, T162:seg3, T189:seg3, T298:seg4]

### Wave 74 — T045, T326, T566
- **任务/area**：T045(platform/web), T326(lib), T566(windows/connection)
- **当前 shard**：T045:seg1, T326:seg2, T566:seg1
- **跨 shard 警告**：wave 内混段（seg1/seg2）；跨段依赖：T326→[T131:seg3, T298:seg4]

### Wave 75 — T527, T568
- **任务/area**：T527(lib), T568(windows/connection)
- **当前 shard**：T527:seg2, T568:seg1
- **跨 shard 警告**：wave 内混段（seg1/seg2）；跨段依赖：T527→[T212:seg3]

### Wave 76 — T313, T349, T531
- **任务/area**：T313(hooks), T349(windows/connection), T531(lib)
- **当前 shard**：T313:seg4, T349:seg1, T531:seg2
- **跨 shard 警告**：wave 内混段（seg1/seg2/seg4）；跨段依赖：T313→[T212:seg3, T525:seg2, T527:seg2]; T349→[T341:seg4]

### Wave 77 — T361, T533
- **任务/area**：T361(windows/connection), T533(lib)
- **当前 shard**：T361:seg1, T533:seg2
- **跨 shard 警告**：wave 内混段（seg1/seg2）；跨段依赖：T361→[T341:seg4]

### Wave 78 — T535, T562
- **任务/area**：T535(lib), T562(windows/connection)
- **当前 shard**：T535:seg2, T562:seg1
- **跨 shard 警告**：wave 内混段（seg1/seg2）；跨段依赖：T562→[T213:seg2]

### Wave 79 — T064, T537, T572
- **任务/area**：T064(components), T537(lib), T572(windows/connection)
- **当前 shard**：T064:seg3, T537:seg2, T572:seg1
- **跨 shard 警告**：wave 内混段（seg1/seg2/seg3）；跨段依赖：T064→[T213:seg2, T341:seg4, T450:seg2, T535:seg2]; T572→[T029:seg4, T213:seg2, T403:seg2, T631:seg2]

### Wave 80 — T134, T264, T539, T576
- **任务/area**：T134(components/DataTable), T264(components), T539(lib), T576(windows/connection)
- **当前 shard**：T134:seg3, T264:seg3, T539:seg2, T576:seg1
- **跨 shard 警告**：wave 内混段（seg1/seg2/seg3）；跨段依赖：T134→[T014:seg2, T326:seg2, T341:seg4, T456:seg2, T491:seg2, T494:seg2, T497:seg2]; T264→[T535:seg2]; T539→[T356:seg4]; T576→[T279:seg4]

### Wave 81 — T284, T409, T541
- **任务/area**：T284(components), T409(windows/connection), T541(lib)
- **当前 shard**：T284:seg3, T409:seg1, T541:seg2
- **跨 shard 警告**：wave 内混段（seg1/seg2/seg3）；跨段依赖：T284→[T278:seg2, T341:seg4]; T409→[T039:seg3, T064:seg3, T213:seg2, T264:seg3, T341:seg4, T450:seg2]; T541→[T386:seg4]

### Wave 82 — T170, T435, T543
- **任务/area**：T170(components/sql-editor), T435(windows/connection), T543(lib)
- **当前 shard**：T170:seg4, T435:seg1, T543:seg2
- **跨 shard 警告**：wave 内混段（seg1/seg2/seg4）；跨段依赖：T170→[T541:seg2]; T435→[T341:seg4, T458:seg3, T541:seg2]

### Wave 83 — T041, T302, T545, T586
- **任务/area**：T041(stores/tableData), T302(components/sql-editor), T545(lib), T586(windows/connection)
- **当前 shard**：T041:seg3, T302:seg4, T545:seg2, T586:seg1
- **跨 shard 警告**：wave 内混段（seg1/seg2/seg3/seg4）；跨段依赖：T041→[T213:seg2, T543:seg2, T629:seg2]; T586→[T631:seg2]

### Wave 84 — T256, T320, T546, T547
- **任务/area**：T256(stores/tableData), T320(components/sql-editor), T546(windows/connection), T547(lib)
- **当前 shard**：T256:seg3, T320:seg4, T546:seg1, T547:seg2
- **跨 shard 警告**：wave 内混段（seg1/seg2/seg3/seg4）；跨段依赖：T256→[T213:seg2, T629:seg2]; T546→[T041:seg3, T129:seg3, T341:seg4, T629:seg2]

### Wave 85 — T263, T549, T592
- **任务/area**：T263(components/sql-editor), T549(lib), T592(windows/connection)
- **当前 shard**：T263:seg4, T549:seg2, T592:seg1
- **跨 shard 警告**：wave 内混段（seg1/seg2/seg4）；跨段依赖：T549→[T331:seg4]; T592→[T272:seg2, T444:seg2]

### Wave 86 — T321, T553, T596
- **任务/area**：T321(windows/settings), T553(lib), T596(windows/connection)
- **当前 shard**：T321:seg4, T553:seg2, T596:seg1
- **跨 shard 警告**：wave 内混段（seg1/seg2/seg4）；跨段依赖：T321→[T255:seg3, T458:seg3, T549:seg2]; T553→[T458:seg3]

### Wave 87 — T555, T600
- **任务/area**：T555(lib), T600(windows/connection)
- **当前 shard**：T555:seg2, T600:seg1
- **跨 shard 警告**：wave 内混段（seg1/seg2）；跨段依赖：T600→[T079:seg3, T091:seg2, T213:seg2]

### Wave 88 — T557, T594
- **任务/area**：T557(lib), T594(windows/connection)
- **当前 shard**：T557:seg2, T594:seg1
- **跨 shard 警告**：wave 内混段（seg1/seg2）；跨段依赖：T557→[T388:seg4]; T594→[T213:seg2]

### Wave 89 — T418, T559
- **任务/area**：T418(windows/connection), T559(lib)
- **当前 shard**：T418:seg1, T559:seg2
- **跨 shard 警告**：wave 内混段（seg1/seg2）；跨段依赖：T418→[T064:seg3, T213:seg2, T341:seg4, T450:seg2, T557:seg2]

### Wave 90 — T376, T449, T561, T602
- **任务/area**：T376(hooks), T449(windows/data-sync), T561(lib), T602(windows/connection)
- **当前 shard**：T376:seg4, T449:seg3, T561:seg2, T602:seg1
- **跨 shard 警告**：wave 内混段（seg1/seg2/seg3/seg4）；跨段依赖：T376→[T559:seg2]; T449→[T341:seg4, T406:seg4, T559:seg2]; T602→[T450:seg2, T535:seg2]

### Wave 91 — T074, T106, T117, T438
- **任务/area**：T074(windows/data-transfer), T106(components/migration), T117(windows/schema-diff), T438(hooks)
- **当前 shard**：T074:seg3, T106:seg1, T117:seg4, T438:seg4
- **跨 shard 警告**：wave 内混段（seg1/seg3/seg4）；跨段依赖：T074→[T341:seg4, T424:seg4, T561:seg2]; T106→[T341:seg4, T561:seg2]; T117→[T561:seg2]; T438→[T559:seg2, T561:seg2]

### Wave 92 — T115, T325, T565
- **任务/area**：T115(windows/data-transfer), T325(windows/schema-diff), T565(lib)
- **当前 shard**：T115:seg3, T325:seg4, T565:seg2
- **跨 shard 警告**：wave 内混段（seg2/seg3/seg4）；跨段依赖：T115→[T106:seg1, T341:seg4, T365:seg4, T424:seg4, T561:seg2]

### Wave 93 — T192, T248, T567
- **任务/area**：T192(components/filter), T248(windows/data-transfer), T567(lib)
- **当前 shard**：T192:seg1, T248:seg3, T567:seg2
- **跨 shard 警告**：wave 内混段（seg1/seg2/seg3）；跨段依赖：T192→[T038:seg4, T213:seg2, T341:seg4, T525:seg2, T531:seg2, T565:seg2]; T248→[T106:seg1, T341:seg4, T365:seg4, T438:seg4, T561:seg2]

### Wave 94 — T155, T260, T512, T569
- **任务/area**：T155(lib/sqlDialects), T260(components/connection), T512(windows/connection), T569(lib)
- **当前 shard**：T155:seg4, T260:seg1, T512:seg1, T569:seg2
- **跨 shard 警告**：wave 内混段（seg1/seg2/seg4）；跨段依赖：T155→[T450:seg2, T541:seg2, T567:seg2, T605:seg2]; T260→[T213:seg2, T341:seg4, T567:seg2]; T512→[T213:seg2, T341:seg4, T458:seg3, T567:seg2]

### Wave 95 — T084, T193, T571
- **任务/area**：T084(lib/sqlDialects), T193(components/connection), T571(lib)
- **当前 shard**：T084:seg4, T193:seg1, T571:seg2
- **跨 shard 警告**：wave 内混段（seg1/seg2/seg4）；跨段依赖：T084→[T095:seg2, T213:seg2, T450:seg2]; T193→[T048:seg4, T341:seg4]

### Wave 96 — T217, T352
- **任务/area**：T217(components/connection), T352(lib)
- **当前 shard**：T217:seg1, T352:seg2
- **跨 shard 警告**：wave 内混段（seg1/seg2）；跨段依赖：T217→[T341:seg4]; T352→[T084:seg4]

### Wave 97 — T028, T194, T281, T573
- **任务/area**：T028(lib/chart), T194(components/chart), T281(components/connection), T573(lib)
- **当前 shard**：T028:seg3, T194:seg2, T281:seg1, T573:seg2
- **跨 shard 警告**：wave 内混段（seg1/seg2/seg3）；跨段依赖：T028→[T298:seg4, T352:seg2]; T194→[T079:seg3, T182:seg3]; T281→[T213:seg2, T341:seg4]

### Wave 98 — T060, T103, T153, T577
- **任务/area**：T060(components/chart), T103(components/connection), T153(lib/chart), T577(lib)
- **当前 shard**：T060:seg2, T103:seg1, T153:seg3, T577:seg2
- **跨 shard 警告**：wave 内混段（seg1/seg2/seg3）；跨段依赖：T060→[T028:seg3, T341:seg4]; T103→[T095:seg2, T213:seg2, T341:seg4, T394:seg2, T450:seg2, T535:seg2]; T153→[T091:seg2, T352:seg2]; T577→[T377:seg3]

### Wave 99 — T168, T300, T579
- **任务/area**：T168(components/chart), T300(components/connection), T579(lib)
- **当前 shard**：T168:seg2, T300:seg1, T579:seg2
- **跨 shard 警告**：wave 内混段（seg1/seg2）；跨段依赖：T168→[T079:seg3, T153:seg3, T182:seg3]; T300→[T091:seg2, T341:seg4, T352:seg2]; T579→[T035:seg4, T084:seg4]

### Wave 100 — T104, T581
- **任务/area**：T104(components/chart), T581(lib)
- **当前 shard**：T104:seg2, T581:seg2
- **跨 shard 警告**：跨段依赖：T104→[T028:seg3, T079:seg3, T153:seg3, T182:seg3, T300:seg1, T341:seg4]

### Wave 101 — T019, T218, T585
- **任务/area**：T019(windows/workflow), T218(components/chart), T585(lib)
- **当前 shard**：T019:seg2, T218:seg2, T585:seg2
- **跨 shard 警告**：跨段依赖：T019→[T079:seg3, T341:seg4]; T218→[T079:seg3, T153:seg3, T182:seg3]

### Wave 102 — T113, T240, T522, T587
- **任务/area**：T113(windows/workflow), T240(components/chart), T522(windows/connection), T587(lib)
- **当前 shard**：T113:seg2, T240:seg2, T522:seg1, T587:seg2
- **跨 shard 警告**：wave 内混段（seg1/seg2）；跨段依赖：T522→[T213:seg2, T341:seg4, T585:seg2]

### Wave 103 — T098, T261, T591
- **任务/area**：T098(components/ui), T261(components/chart), T591(lib)
- **当前 shard**：T098:seg3, T261:seg2, T591:seg2
- **跨 shard 警告**：wave 内混段（seg2/seg3）；跨段依赖：T098→[T029:seg4, T567:seg2, T587:seg2]; T261→[T079:seg3]; T591→[T379:seg4]

### Wave 104 — T025, T593
- **任务/area**：T025(windows/schema-diff), T593(lib)
- **当前 shard**：T025:seg4, T593:seg2
- **跨 shard 警告**：wave 内混段（seg2/seg4）；跨段依赖：T025→[T591:seg2]

### Wave 105 — T206, T595
- **任务/area**：T206(windows/schema-diff), T595(lib)
- **当前 shard**：T206:seg4, T595:seg2
- **跨 shard 警告**：wave 内混段（seg2/seg4）

### Wave 106 — T076, T597
- **任务/area**：T076(windows/schema-diff), T597(lib)
- **当前 shard**：T076:seg4, T597:seg2
- **跨 shard 警告**：wave 内混段（seg2/seg4）；跨段依赖：T076→[T213:seg2]

### Wave 107 — T599
- **任务/area**：T599(lib)
- **当前 shard**：T599:seg2
- **跨 shard 警告**：无

### Wave 108 — T601
- **任务/area**：T601(lib)
- **当前 shard**：T601:seg2
- **跨 shard 警告**：无

### Wave 109 — T528, T603
- **任务/area**：T528(windows/connection), T603(lib)
- **当前 shard**：T528:seg1, T603:seg2
- **跨 shard 警告**：wave 内混段（seg1/seg2）；跨段依赖：T528→[T091:seg2, T279:seg4, T341:seg4, T482:seg2, T601:seg2]

### Wave 110 — T462
- **任务/area**：T462(lib)
- **当前 shard**：T462:seg2
- **跨 shard 警告**：无

### Wave 111 — T328, T605
- **任务/area**：T328(hooks), T605(lib)
- **当前 shard**：T328:seg4, T605:seg2
- **跨 shard 警告**：wave 内混段（seg2/seg4）；跨段依赖：T328→[T212:seg3, T462:seg2, T525:seg2]

### Wave 112 — T607
- **任务/area**：T607(lib)
- **当前 shard**：T607:seg2
- **跨 shard 警告**：无

### Wave 113 — T359, T613
- **任务/area**：T359(components/sql-editor), T613(lib)
- **当前 shard**：T359:seg4, T613:seg2
- **跨 shard 警告**：wave 内混段（seg2/seg4）；跨段依赖：T359→[T607:seg2]; T613→[T131:seg3]

### Wave 114 — T347, T382, T615
- **任务/area**：T347(components/sql-editor), T382(windows/connection), T615(lib)
- **当前 shard**：T347:seg4, T382:seg1, T615:seg2
- **跨 shard 警告**：wave 内混段（seg1/seg2/seg4）；跨段依赖：T382→[T038:seg4, T077:seg3, T310:seg2, T341:seg4, T613:seg2]

### Wave 115 — T220, T426, T516, T609
- **任务/area**：T220(components/sql-editor), T426(windows/settings), T516(windows/connection), T609(lib)
- **当前 shard**：T220:seg4, T426:seg4, T516:seg1, T609:seg2
- **跨 shard 警告**：wave 内混段（seg1/seg2/seg4）；跨段依赖：T220→[T213:seg2, T615:seg2]; T426→[T213:seg2, T327:seg1, T458:seg3]; T516→[T243:seg3, T341:seg4, T347:seg4, T458:seg3, T567:seg2]

### Wave 116 — T617
- **任务/area**：T617(lib)
- **当前 shard**：T617:seg2
- **跨 shard 警告**：跨段依赖：T617→[T084:seg4]

### Wave 117 — T340, T439, T582
- **任务/area**：T340(lib), T439(stores), T582(windows/connection)
- **当前 shard**：T340:seg2, T439:seg3, T582:seg1
- **跨 shard 警告**：wave 内混段（seg1/seg2/seg3）；跨段依赖：T439→[T213:seg2, T567:seg2, T617:seg2]; T582→[T030:seg3, T213:seg2, T450:seg2, T617:seg2]

### Wave 118 — T403, T405
- **任务/area**：T403(lib), T405(stores)
- **当前 shard**：T403:seg2, T405:seg3
- **跨 shard 警告**：wave 内混段（seg2/seg3）；跨段依赖：T403→[T039:seg3, T431:seg3]; T405→[T311:seg1, T477:seg2]

### Wave 119 — T039, T611
- **任务/area**：T039(stores), T611(lib)
- **当前 shard**：T039:seg3, T611:seg2
- **跨 shard 警告**：wave 内混段（seg2/seg3）；跨段依赖：T039→[T190:seg4, T213:seg2, T437:seg2]

### Wave 120 — T127, T570, T619
- **任务/area**：T127(stores), T570(windows/connection), T619(lib)
- **当前 shard**：T127:seg3, T570:seg1, T619:seg2
- **跨 shard 警告**：wave 内混段（seg1/seg2/seg3）；跨段依赖：T127→[T190:seg4, T213:seg2, T403:seg2, T437:seg2]; T570→[T039:seg3, T213:seg2, T394:seg2, T403:seg2, T431:seg3, T450:seg2, T567:seg2, T617:seg2]

### Wave 121 — T181, T314, T333, T348
- **任务/area**：T181(lib), T314(stores), T333(components/connection), T348(windows/settings)
- **当前 shard**：T181:seg2, T314:seg3, T333:seg1, T348:seg4
- **跨 shard 警告**：wave 内混段（seg1/seg2/seg3/seg4）；跨段依赖：T181→[T096:seg4]; T314→[T208:seg1, T577:seg2]; T333→[T095:seg2, T127:seg3, T190:seg4, T213:seg2, T341:seg4, T394:seg2, T450:seg2, T571:seg2]; T348→[T127:seg3, T142:seg3, T213:seg2, T458:seg3, T557:seg2]

### Wave 122 — T487, T574
- **任务/area**：T487(lib), T574(windows/connection)
- **当前 shard**：T487:seg2, T574:seg1
- **跨 shard 警告**：wave 内混段（seg1/seg2）；跨段依赖：T574→[T127:seg3, T190:seg4, T403:seg2, T437:seg2]

### Wave 123 — T575
- **任务/area**：T575(lib)
- **当前 shard**：T575:seg2
- **跨 shard 警告**：无

### Wave 124 — T102, T598, T621
- **任务/area**：T102(components/query), T598(windows/connection), T621(lib)
- **当前 shard**：T102:seg3, T598:seg1, T621:seg2
- **跨 shard 警告**：wave 内混段（seg1/seg2/seg3）；跨段依赖：T102→[T341:seg4, T575:seg2, T617:seg2]; T598→[T233:seg4, T575:seg2, T605:seg2, T617:seg2]

### Wave 125 — T589
- **任务/area**：T589(lib)
- **当前 shard**：T589:seg2
- **跨 shard 警告**：跨段依赖：T589→[T311:seg1]

### Wave 126 — T422, T503
- **任务/area**：T422(hooks), T503(lib)
- **当前 shard**：T422:seg4, T503:seg2
- **跨 shard 警告**：wave 内混段（seg2/seg4）；跨段依赖：T422→[T515:seg2, T589:seg2]; T503→[T084:seg4]

### Wave 127 — T623
- **任务/area**：T623(lib)
- **当前 shard**：T623:seg2
- **跨 shard 警告**：跨段依赖：T623→[T131:seg3]

### Wave 128 — T627
- **任务/area**：T627(lib)
- **当前 shard**：T627:seg2
- **跨 shard 警告**：无

### Wave 129 — T625
- **任务/area**：T625(lib)
- **当前 shard**：T625:seg2
- **跨 shard 警告**：跨段依赖：T625→[T406:seg4]

### Wave 130 — T629
- **任务/area**：T629(lib)
- **当前 shard**：T629:seg2
- **跨 shard 警告**：无

### Wave 131 — T129, T258, T299, T529
- **任务/area**：T129(stores/tableData), T258(commands), T299(components/DataTable), T529(lib)
- **当前 shard**：T129:seg3, T258:seg4, T299:seg3, T529:seg2
- **跨 shard 警告**：wave 内混段（seg2/seg3/seg4）；跨段依赖：T129→[T213:seg2, T629:seg2]; T258→[T213:seg2, T311:seg1, T327:seg1, T629:seg2]; T299→[T213:seg2, T341:seg4, T525:seg2, T629:seg2]

### Wave 132 — T219, T235, T280, T286
- **任务/area**：T219(components/ai), T235(stores/tableData), T280(components/DataTable), T286(windows/connection)
- **当前 shard**：T219:seg3, T235:seg3, T280:seg3, T286:seg1
- **跨 shard 警告**：wave 内混段（seg1/seg3）；跨段依赖：T219→[T213:seg2, T214:seg4, T258:seg4, T341:seg4]; T235→[T213:seg2, T543:seg2, T629:seg2]; T280→[T341:seg4]; T286→[T065:seg3, T254:seg4, T258:seg4, T279:seg4, T341:seg4, T364:seg2, T491:seg2]

### Wave 133 — T259, T277, T344, T351
- **任务/area**：T259(components/DataTable), T277(stores/tableData), T344(commands), T351(windows/schema-diff)
- **当前 shard**：T259:seg3, T277:seg3, T344:seg4, T351:seg4
- **跨 shard 警告**：wave 内混段（seg3/seg4）；跨段依赖：T259→[T157:seg4, T213:seg2, T341:seg4]; T277→[T213:seg2, T629:seg2]; T344→[T192:seg1]; T351→[T327:seg1]

### Wave 134 — T343, T372, T380, T411
- **任务/area**：T343(stores/tableData), T372(windows/connection), T380(components/DataTable), T411(windows/schema-diff)
- **当前 shard**：T343:seg3, T372:seg1, T380:seg3, T411:seg4
- **跨 shard 警告**：wave 内混段（seg1/seg3/seg4）；跨段依赖：T343→[T213:seg2, T629:seg2]; T372→[T028:seg3, T033:seg2, T082:seg2, T213:seg2, T258:seg4, T341:seg4, T458:seg3, T487:seg2, T567:seg2]; T380→[T298:seg4, T310:seg2, T341:seg4]; T411→[T213:seg2]

### Wave 135 — T238, T330, T420, T431
- **任务/area**：T238(components/DataTable), T330(stores/tableData), T420(windows/schema-diff), T431(stores)
- **当前 shard**：T238:seg3, T330:seg3, T420:seg4, T431:seg3
- **跨 shard 警告**：wave 内混段（seg3/seg4）；跨段依赖：T238→[T157:seg4, T158:seg4, T341:seg4, T445:seg4]; T330→[T213:seg2, T629:seg2]; T420→[T213:seg2]; T431→[T081:seg1, T213:seg2, T258:seg4, T311:seg1, T450:seg2, T487:seg2, T617:seg2]

### Wave 136 — T090, T329, T391, T398
- **任务/area**：T090(stores/tableData), T329(stores), T391(windows/connection), T398(components/DataTable)
- **当前 shard**：T090:seg3, T329:seg3, T391:seg1, T398:seg3
- **跨 shard 警告**：wave 内混段（seg1/seg3）；跨段依赖：T090→[T213:seg2, T257:seg2, T585:seg2, T629:seg2]; T329→[T450:seg2, T575:seg2, T617:seg2]; T391→[T014:seg2, T064:seg3, T162:seg3, T213:seg2, T326:seg2, T341:seg4, T431:seg3, T456:seg2, T491:seg2, T494:seg2, T497:seg2, T589:seg2]; T398→[T157:seg4, T213:seg2, T341:seg4, T457:seg4, T515:seg2]

### Wave 137 — T355, T400, T446, T467
- **任务/area**：T355(stores/tableData), T400(windows/connection), T446(stores), T467(lib)
- **当前 shard**：T355:seg3, T400:seg1, T446:seg3, T467:seg2
- **跨 shard 警告**：wave 内混段（seg1/seg2/seg3）；跨段依赖：T355→[T213:seg2, T525:seg2, T629:seg2]; T400→[T213:seg2, T341:seg4, T431:seg3, T589:seg2]; T446→[T081:seg1, T213:seg2, T450:seg2]; T467→[T190:seg4, T258:seg4]

### Wave 138 — T296, T442, T476, T507
- **任务/area**：T296(stores/tableData), T442(windows/connection), T476(windows/data-sync), T507(lib)
- **当前 shard**：T296:seg3, T442:seg1, T476:seg3, T507:seg2
- **跨 shard 警告**：wave 内混段（seg1/seg2/seg3）；跨段依赖：T296→[T213:seg2, T629:seg2]; T442→[T298:seg4, T341:seg4, T431:seg3, T537:seg2, T539:seg2, T541:seg2, T589:seg2]; T476→[T213:seg2, T316:seg4, T341:seg4, T406:seg4, T467:seg2]; T507→[T084:seg4, T258:seg4]

### Wave 139 — T126, T378, T433, T448
- **任务/area**：T126(hooks), T378(stores/tableData), T433(components/DataTable), T448(windows/connection)
- **当前 shard**：T126:seg4, T378:seg3, T433:seg3, T448:seg1
- **跨 shard 警告**：wave 内混段（seg1/seg3/seg4）；跨段依赖：T126→[T507:seg2]; T378→[T213:seg2, T543:seg2, T629:seg2]; T433→[T213:seg2, T462:seg2, T507:seg2, T523:seg2, T525:seg2, T567:seg2, T603:seg2]; T448→[T084:seg4, T213:seg2, T258:seg4, T341:seg4, T431:seg3, T450:seg2, T458:seg3, T589:seg2]

### Wave 140 — T139, T165, T294, T463
- **任务/area**：T139(components/ai), T165(components/DataTable), T294(hooks), T463(stores)
- **当前 shard**：T139:seg3, T165:seg3, T294:seg4, T463:seg3
- **跨 shard 警告**：wave 内混段（seg3/seg4）；跨段依赖：T139→[T126:seg4, T213:seg2, T341:seg4]; T165→[T014:seg2, T157:seg4, T158:seg4, T185:seg4, T213:seg2, T313:seg4, T328:seg4, T341:seg4, T445:seg4, T515:seg2, T523:seg2]; T294→[T212:seg3, T213:seg2, T296:seg3, T416:seg3, T523:seg2, T525:seg2, T529:seg2, T629:seg2]; T463→[T213:seg2, T257:seg2, T258:seg4, T292:seg1, T450:seg2, T472:seg2, T477:seg2, T629:seg2]

### Wave 141 — T069, T377, T470, T551
- **任务/area**：T069(windows/dashboard), T377(stores), T470(windows/connection), T551(lib)
- **当前 shard**：T069:seg4, T377:seg3, T470:seg1, T551:seg2
- **跨 shard 警告**：wave 内混段（seg1/seg2/seg3/seg4）；跨段依赖：T069→[T030:seg3, T079:seg3, T091:seg2, T161:seg2, T165:seg3, T300:seg1]; T377→[T091:seg2, T213:seg2, T252:seg1, T437:seg2, T577:seg2, T579:seg2, T589:seg2]; T470→[T039:seg3, T064:seg3, T213:seg2, T264:seg3, T403:seg2, T431:seg3, T450:seg2, T477:seg2]; T551→[T084:seg4, T258:seg4, T311:seg1, T431:seg3]

### Wave 142 — T175, T354, T495, T583
- **任务/area**：T175(windows/dashboard), T354(stores), T495(windows/connection), T583(lib)
- **当前 shard**：T175:seg4, T354:seg3, T495:seg1, T583:seg2
- **跨 shard 警告**：wave 内混段（seg1/seg2/seg3/seg4）；跨段依赖：T175→[T030:seg3, T161:seg2, T165:seg3]; T354→[T091:seg2, T208:seg1, T213:seg2, T292:seg1, T356:seg4, T577:seg2]; T495→[T038:seg4, T065:seg3, T213:seg2, T254:seg4, T258:seg4, T279:seg4, T341:seg4, T356:seg4, T364:seg2]

### Wave 143 — T223, T342, T395, T517
- **任务/area**：T223(windows/connection), T342(stores), T395(hooks), T517(lib)
- **当前 shard**：T223:seg1, T342:seg3, T395:seg4, T517:seg2
- **跨 shard 警告**：wave 内混段（seg1/seg2/seg3/seg4）；跨段依赖：T223→[T213:seg2, T354:seg3, T431:seg3, T491:seg2, T551:seg2]; T342→[T356:seg4, T583:seg2]; T395→[T276:seg3, T354:seg3]; T517→[T234:seg3, T258:seg4, T354:seg3, T366:seg3, T463:seg3]

### Wave 144 — T498, T519
- **任务/area**：T498(windows/connection), T519(lib)
- **当前 shard**：T498:seg1, T519:seg2
- **跨 shard 警告**：wave 内混段（seg1/seg2）；跨段依赖：T498→[T038:seg4, T165:seg3, T213:seg2, T279:seg4, T341:seg4, T573:seg2]; T519→[T234:seg3, T258:seg4, T354:seg3, T366:seg3, T463:seg3]

### Wave 145 — T504, T631
- **任务/area**：T504(windows/connection), T631(lib)
- **当前 shard**：T504:seg1, T631:seg2
- **跨 shard 警告**：wave 内混段（seg1/seg2）；跨段依赖：T504→[T033:seg2, T082:seg2, T213:seg2, T431:seg3, T450:seg2, T458:seg3, T589:seg2, T615:seg2]; T631→[T311:seg1]

### Wave 146 — T460, T632
- **任务/area**：T460(windows/connection), T632(lib)
- **当前 shard**：T460:seg1, T632:seg2
- **跨 shard 警告**：wave 内混段（seg1/seg2）；跨段依赖：T460→[T213:seg2, T341:seg4, T595:seg2, T631:seg2]

### Wave 147 — T141, T221, T508, T633
- **任务/area**：T141(components/sql-editor), T221(components), T508(windows/connection), T633(lib)
- **当前 shard**：T141:seg4, T221:seg3, T508:seg1, T633:seg2
- **跨 shard 警告**：wave 内混段（seg1/seg2/seg3/seg4）；跨段依赖：T141→[T081:seg1, T121:seg2, T458:seg3, T541:seg2, T607:seg2, T609:seg2, T619:seg2, T621:seg2, T632:seg2]; T221→[T632:seg2]; T508→[T341:seg4, T354:seg3, T356:seg4, T366:seg3, T431:seg3, T531:seg2, T567:seg2, T575:seg2, T579:seg2, T617:seg2]; T633→[T131:seg3, T440:seg4]

### Wave 148 — T011, T222, T322, T430
- **任务/area**：T011(components/sql-editor), T222(windows/settings), T322(windows/connection), T430(hooks)
- **当前 shard**：T011:seg4, T222:seg4, T322:seg1, T430:seg4
- **跨 shard 警告**：wave 内混段（seg1/seg4）；跨段依赖：T011→[T458:seg3, T619:seg2]; T222→[T213:seg2, T278:seg2, T297:seg2, T458:seg3, T483:seg3, T633:seg2]; T322→[T213:seg2, T221:seg3, T341:seg4, T450:seg2, T507:seg2, T567:seg2]; T430→[T278:seg2, T437:seg2, T458:seg3, T633:seg2]

### Wave 149 — T243, T404, T458, T514
- **任务/area**：T243(components), T404(hooks), T458(stores), T514(windows/connection)
- **当前 shard**：T243:seg3, T404:seg4, T458:seg3, T514:seg1
- **跨 shard 警告**：wave 内混段（seg1/seg3/seg4）；跨段依赖：T243→[T011:seg4, T107:seg4, T632:seg2]; T458→[T161:seg2, T184:seg4, T213:seg2, T257:seg2, T278:seg2, T312:seg4, T388:seg4, T437:seg2, T581:seg2, T615:seg2, T633:seg2]; T514→[T341:seg4, T431:seg3, T589:seg2]

### Wave 150 — T070, T201, T239, T305
- **任务/area**：T070(windows/workflow), T201(windows/dashboard), T239(components/connection), T305(windows/connection)
- **当前 shard**：T070:seg2, T201:seg4, T239:seg1, T305:seg1
- **跨 shard 警告**：wave 内混段（seg1/seg2/seg4）；跨段依赖：T070→[T039:seg3, T096:seg4, T155:seg4, T243:seg3, T279:seg4, T341:seg4, T484:seg4]; T201→[T091:seg2, T127:seg3, T161:seg2, T213:seg2, T243:seg3]; T239→[T064:seg3, T127:seg3, T190:seg4, T213:seg2, T341:seg4, T385:seg2, T394:seg2, T404:seg4, T450:seg2, T458:seg3, T509:seg2]; T305→[T213:seg2, T243:seg3, T258:seg4, T341:seg4, T356:seg4]

### Wave 151 — T225, T337, T634
- **任务/area**：T225(windows/workflow), T337(windows/connection), T634(lib)
- **当前 shard**：T225:seg2, T337:seg1, T634:seg2
- **跨 shard 警告**：wave 内混段（seg1/seg2）；跨段依赖：T337→[T029:seg4, T185:seg4, T213:seg2, T243:seg3, T258:seg4, T341:seg4, T354:seg3, T395:seg4, T431:seg3]

### Wave 152 — T421, T480
- **任务/area**：T421(lib), T480(windows/connection)
- **当前 shard**：T421:seg2, T480:seg1
- **跨 shard 警告**：wave 内混段（seg1/seg2）；跨段依赖：T421→[T029:seg4, T337:seg1]; T480→[T030:seg3, T213:seg2, T243:seg3, T258:seg4, T341:seg4, T356:seg4, T507:seg2, T567:seg2, T615:seg2]

### Wave 153 — T346, T524, T635
- **任务/area**：T346(components/connection), T524(windows/connection), T635(lib)
- **当前 shard**：T346:seg1, T524:seg1, T635:seg2
- **跨 shard 警告**：wave 内混段（seg1/seg2）；跨段依赖：T346→[T190:seg4, T341:seg4, T421:seg2, T535:seg2]; T524→[T129:seg3, T165:seg3, T213:seg2, T257:seg2, T341:seg4, T342:seg3, T356:seg4, T416:seg3, T451:seg4, T458:seg3, T491:seg2, T583:seg2, T585:seg2]

### Wave 154 — T137, T526, T636
- **任务/area**：T137(components/connection), T526(windows/connection), T636(lib)
- **当前 shard**：T137:seg1, T526:seg1, T636:seg2
- **跨 shard 警告**：wave 内混段（seg1/seg2）；跨段依赖：T137→[T190:seg4, T341:seg4, T421:seg2]; T526→[T091:seg2, T104:seg2, T213:seg2, T341:seg4, T491:seg2]

### Wave 155 — T247, T412, T534
- **任务/area**：T247(windows/onboarding), T412(lib), T534(windows/connection)
- **当前 shard**：T247:seg4, T412:seg2, T534:seg1
- **跨 shard 警告**：wave 内混段（seg1/seg2/seg4）；跨段依赖：T247→[T137:seg1]; T412→[T137:seg1]; T534→[T213:seg2, T341:seg4, T431:seg3, T589:seg2]

### Wave 156 — T167, T177, T536, T637
- **任务/area**：T167(components/connection), T177(windows/onboarding), T536(windows/connection), T637(lib)
- **当前 shard**：T167:seg1, T177:seg4, T536:seg1, T637:seg2
- **跨 shard 警告**：wave 内混段（seg1/seg2/seg4）；跨段依赖：T167→[T127:seg3, T341:seg4, T412:seg2, T421:seg2]; T177→[T103:seg1, T137:seg1, T346:seg1]; T536→[T294:seg4, T341:seg4]

### Wave 157 — T203, T447, T550, T638
- **任务/area**：T203(windows/onboarding), T447(windows/settings), T550(windows/connection), T638(lib)
- **当前 shard**：T203:seg4, T447:seg4, T550:seg1, T638:seg2
- **跨 shard 警告**：wave 内混段（seg1/seg2/seg4）；跨段依赖：T203→[T064:seg3, T103:seg1, T127:seg3, T213:seg2, T318:seg1, T333:seg1, T450:seg2, T509:seg2]; T447→[T213:seg2, T217:seg1, T281:seg1, T300:seg1, T318:seg1, T468:seg3, T571:seg2, T637:seg2]; T550→[T038:seg4, T213:seg2, T341:seg4, T397:seg4, T431:seg3, T450:seg2, T482:seg2, T491:seg2, T503:seg2, T589:seg2]; T638→[T131:seg3, T162:seg3]

### Wave 158 — T226, T453, T478, T560
- **任务/area**：T226(windows/onboarding), T453(windows/settings), T478(stores), T560(windows/connection)
- **当前 shard**：T226:seg4, T453:seg4, T478:seg3, T560:seg1
- **跨 shard 警告**：wave 内混段（seg1/seg3/seg4）；跨段依赖：T226→[T127:seg3, T260:seg1, T394:seg2]; T453→[T127:seg3, T213:seg2, T468:seg3, T636:seg2, T637:seg2]; T478→[T638:seg2]; T560→[T039:seg3, T127:seg3, T264:seg3, T354:seg3]

### Wave 159 — T144, T464, T639
- **任务/area**：T144(windows/connection), T464(windows/settings), T639(lib)
- **当前 shard**：T144:seg1, T464:seg4, T639:seg2
- **跨 shard 警告**：wave 内混段（seg1/seg2/seg4）；跨段依赖：T144→[T039:seg3, T064:seg3, T127:seg3, T213:seg2, T264:seg3, T341:seg4, T354:seg3, T356:seg4, T450:seg2]; T464→[T638:seg2]; T639→[T478:seg3]

### Wave 160 — T489, T640
- **任务/area**：T489(windows/connection), T640(lib)
- **当前 shard**：T489:seg1, T640:seg2
- **跨 shard 警告**：wave 内混段（seg1/seg2）；跨段依赖：T489→[T341:seg4, T354:seg3, T431:seg3]; T640→[T258:seg4]

### Wave 161 — T291, T454
- **任务/area**：T291(lib), T454(windows/connection)
- **当前 shard**：T291:seg2, T454:seg1
- **跨 shard 警告**：wave 内混段（seg1/seg2）；跨段依赖：T454→[T213:seg2, T341:seg4, T589:seg2, T640:seg2]

### Wave 162 — T040, T143, T230, T556
- **任务/area**：T040(stores/ai), T143(windows/settings), T230(lib), T556(windows/connection)
- **当前 shard**：T040:seg2, T143:seg4, T230:seg2, T556:seg1
- **跨 shard 警告**：wave 内混段（seg1/seg2/seg4）；跨段依赖：T040→[T039:seg3, T096:seg4]; T143→[T213:seg2, T251:seg2, T291:seg2]; T556→[T038:seg4, T264:seg3, T341:seg4, T478:seg3, T535:seg2]

### Wave 163 — T061, T089, T584, T641
- **任务/area**：T061(components/ai), T089(stores/ai), T584(windows/connection), T641(lib)
- **当前 shard**：T061:seg3, T089:seg2, T584:seg1, T641:seg2
- **跨 shard 警告**：wave 内混段（seg1/seg2/seg3）；跨段依赖：T061→[T230:seg2, T341:seg4]; T089→[T096:seg4, T097:seg1, T133:seg1, T164:seg1]; T584→[T039:seg3, T127:seg3, T213:seg2, T354:seg3, T366:seg3, T450:seg2, T575:seg2]

### Wave 164 — T017, T052, T088, T169
- **任务/area**：T017(windows/workspace), T052(index.ts), T088(stores), T169(components/ai)
- **当前 shard**：T017:seg4, T052:seg2, T088:seg3, T169:seg3
- **跨 shard 警告**：wave 内混段（seg2/seg3/seg4）；跨段依赖：T017→[T641:seg2]; T052→[T092:seg3, T131:seg3, T162:seg3]; T088→[T040:seg2, T089:seg2, T096:seg4, T128:seg2, T213:seg2, T230:seg2, T251:seg2, T272:seg2, T291:seg2]; T169→[T230:seg2, T533:seg2]

### Wave 165 — T015, T111, T148, T195
- **任务/area**：T015(windows/settings), T111(windows/workspace), T148(windows/onboarding), T195(components/ai)
- **当前 shard**：T015:seg4, T111:seg4, T148:seg4, T195:seg3
- **跨 shard 警告**：wave 内混段（seg3/seg4）；跨段依赖：T015→[T088:seg3, T213:seg2]; T148→[T088:seg3]; T195→[T341:seg4]

### Wave 166 — T145, T336, T588, T643
- **任务/area**：T145(windows/workspace), T336(windows/settings), T588(windows/connection), T643(lib)
- **当前 shard**：T145:seg4, T336:seg4, T588:seg1, T643:seg2
- **跨 shard 警告**：wave 内混段（seg1/seg2/seg4）；跨段依赖：T145→[T488:seg3]; T336→[T088:seg3, T142:seg3, T213:seg2, T458:seg3, T571:seg2]; T588→[T039:seg3, T155:seg4, T243:seg3, T272:seg2, T276:seg3, T354:seg3, T377:seg3, T405:seg3, T431:seg3, T452:seg3, T605:seg2]; T643→[T440:seg4]

### Wave 167 — T174, T360, T518, T642
- **任务/area**：T174(windows/workspace), T360(windows/settings), T518(windows/connection), T642(lib)
- **当前 shard**：T174:seg4, T360:seg4, T518:seg1, T642:seg2
- **跨 shard 警告**：wave 内混段（seg1/seg2/seg4）；跨段依赖：T174→[T488:seg3]; T360→[T088:seg3, T127:seg3, T213:seg2, T230:seg2]; T518→[T038:seg4, T127:seg3, T155:seg4, T243:seg3, T276:seg3, T341:seg4, T354:seg3, T377:seg3, T394:seg2, T458:seg3, T605:seg2, T621:seg2]; T642→[T039:seg3, T127:seg3, T190:seg4, T440:seg4]

### Wave 168 — T068, T408, T604, T644
- **任务/area**：T068(windows/workspace), T408(windows/settings), T604(windows/connection), T644(lib)
- **当前 shard**：T068:seg4, T408:seg4, T604:seg1, T644:seg2
- **跨 shard 警告**：wave 内混段（seg1/seg2/seg4）；跨段依赖：T068→[T297:seg2, T458:seg3, T483:seg3, T488:seg3, T641:seg2, T642:seg2]; T408→[T088:seg3, T213:seg2, T291:seg2, T571:seg2]; T604→[T431:seg3]

### Wave 169 — T012, T026, T200, T479
- **任务/area**：T012(components), T026(windows/backup), T200(windows/workspace), T479(windows/settings)
- **当前 shard**：T012:seg3, T026:seg3, T200:seg4, T479:seg4
- **跨 shard 警告**：wave 内混段（seg3/seg4）；跨段依赖：T012→[T096:seg4, T213:seg2, T341:seg4, T644:seg2]; T026→[T038:seg4, T084:seg4, T213:seg2, T310:seg2, T341:seg4, T394:seg2, T404:seg4, T437:seg2, T450:seg2, T613:seg2, T644:seg2]; T200→[T131:seg3, T483:seg3, T488:seg3]; T479→[T088:seg3, T213:seg2, T251:seg2, T291:seg2]

### Wave 170 — T066, T171, T563, T580
- **任务/area**：T066(windows/settings), T171(components), T563(lib), T580(windows/connection)
- **当前 shard**：T066:seg4, T171:seg3, T563:seg2, T580:seg1
- **跨 shard 警告**：wave 内混段（seg1/seg2/seg3/seg4）；跨段依赖：T066→[T213:seg2]; T171→[T096:seg4, T213:seg2, T341:seg4, T644:seg2]; T580→[T039:seg3, T190:seg4, T213:seg2, T258:seg4, T431:seg3, T439:seg3, T450:seg2]

### Wave 171 — T149, T172, T384, T443
- **任务/area**：T149(windows/data-transfer), T172(windows/settings), T384(windows/schema-diff), T443(windows/data-sync)
- **当前 shard**：T149:seg3, T172:seg4, T384:seg4, T443:seg3
- **跨 shard 警告**：wave 内混段（seg3/seg4）；跨段依赖：T149→[T140:seg1, T190:seg4, T213:seg2, T341:seg4, T365:seg4, T376:seg4, T404:seg4, T415:seg4, T437:seg2, T438:seg4, T450:seg2, T467:seg2, T563:seg2, T636:seg2]; T172→[T088:seg3, T213:seg2, T458:seg3, T571:seg2]; T384→[T213:seg2, T437:seg2, T450:seg2, T467:seg2, T482:seg2, T502:seg3, T563:seg2]; T443→[T213:seg2, T258:seg4, T341:seg4, T437:seg2, T450:seg2, T467:seg2, T482:seg2, T563:seg2, T625:seg2]

### Wave 172 — T390, T606, T645
- **任务/area**：T390(windows/settings), T606(windows/connection), T645(lib)
- **当前 shard**：T390:seg4, T606:seg1, T645:seg2
- **跨 shard 警告**：wave 内混段（seg1/seg2/seg4）；跨段依赖：T390→[T131:seg3, T213:seg2, T243:seg3, T255:seg3, T264:seg3, T458:seg3, T531:seg2, T541:seg2, T571:seg2, T632:seg2]; T606→[T212:seg3, T213:seg2, T258:seg4, T352:seg2, T356:seg4, T407:seg3, T450:seg2, T463:seg3, T521:seg2, T603:seg2]; T645→[T131:seg3, T162:seg3, T388:seg4, T458:seg3]

### Wave 173 — T112, T114, T173, T197
- **任务/area**：T112(windows/dashboard), T114(windows/onboarding), T173(windows/connection), T197(components)
- **当前 shard**：T112:seg4, T114:seg4, T173:seg1, T197:seg3
- **跨 shard 警告**：wave 内混段（seg1/seg3/seg4）；跨段依赖：T112→[T091:seg2, T142:seg3, T161:seg2, T186:seg3, T213:seg2, T488:seg3, T645:seg2]; T114→[T088:seg3, T137:seg1, T458:seg3, T645:seg2]; T173→[T213:seg2, T233:seg4, T259:seg3, T341:seg4, T353:seg4, T386:seg4, T450:seg2, T541:seg2, T545:seg2, T635:seg2, T645:seg2]; T197→[T278:seg2, T341:seg4, T386:seg4, T388:seg4, T437:seg2, T573:seg2, T645:seg2]

### Wave 174 — T072, T241, T270, T309
- **任务/area**：T072(windows/onboarding), T241(components/ai), T270(windows/data-sync), T309(windows/schema-diff)
- **当前 shard**：T072:seg4, T241:seg3, T270:seg3, T309:seg4
- **跨 shard 警告**：wave 内混段（seg3/seg4）；跨段依赖：T241→[T272:seg2, T341:seg4, T645:seg2]; T270→[T140:seg1, T341:seg4, T365:seg4, T376:seg4, T404:seg4, T645:seg2]; T309→[T140:seg1, T142:seg3, T213:seg2, T264:seg3, T458:seg3, T559:seg2, T591:seg2, T593:seg2, T645:seg2]

### Wave 175 — T262, T558, T646
- **任务/area**：T262(components/ai), T558(windows/connection), T646(lib)
- **当前 shard**：T262:seg3, T558:seg1, T646:seg2
- **跨 shard 警告**：wave 内混段（seg1/seg2/seg3）；跨段依赖：T262→[T213:seg2, T341:seg4, T645:seg2]; T558→[T213:seg2, T431:seg3, T645:seg2]

### Wave 176 — T282, T510, T647
- **任务/area**：T282(components/ai), T510(windows/connection), T647(lib)
- **当前 shard**：T282:seg3, T510:seg1, T647:seg2
- **跨 shard 警告**：wave 内混段（seg1/seg2/seg3）；跨段依赖：T282→[T029:seg4, T096:seg4, T213:seg2, T230:seg2, T341:seg4, T645:seg2]; T510→[T018:seg4, T058:seg3, T091:seg2, T102:seg3, T134:seg3, T213:seg2, T241:seg3, T262:seg3, T272:seg2, T341:seg4, T342:seg3, T450:seg2, T491:seg2, T621:seg2]

### Wave 177 — T301, T506
- **任务/area**：T301(components/ai), T506(windows/connection)
- **当前 shard**：T301:seg3, T506:seg1
- **跨 shard 警告**：wave 内混段（seg1/seg3）；跨段依赖：T301→[T126:seg4, T341:seg4, T645:seg2]; T506→[T102:seg3, T136:seg3, T155:seg4, T242:seg4, T243:seg3, T282:seg3, T341:seg4, T386:seg4, T458:seg3, T541:seg2, T567:seg2, T577:seg2, T605:seg2, T617:seg2]

### Wave 178 — T334, T540
- **任务/area**：T334(components/ai), T540(windows/connection)
- **当前 shard**：T334:seg3, T540:seg1
- **跨 shard 警告**：wave 内混段（seg1/seg3）；跨段依赖：T334→[T019:seg2, T038:seg4, T070:seg2, T096:seg4, T113:seg2, T190:seg4, T213:seg2, T225:seg2, T341:seg4, T365:seg4, T645:seg2]; T540→[T213:seg2, T301:seg3, T341:seg4, T511:seg2, T565:seg2]

### Wave 179 — T009, T552
- **任务/area**：T009(components/ai), T552(windows/connection)
- **当前 shard**：T009:seg3, T552:seg1
- **跨 shard 警告**：wave 内混段（seg1/seg3）；跨段依赖：T009→[T029:seg4, T040:seg2, T087:seg4, T096:seg4, T126:seg4, T213:seg2, T230:seg2, T291:seg2, T341:seg4, T545:seg2, T586:seg1, T645:seg2]; T552→[T213:seg2, T341:seg4]

### Wave 180 — T245, T319
- **任务/area**：T245(windows/connection), T319(components/ai)
- **当前 shard**：T245:seg1, T319:seg3
- **跨 shard 警告**：wave 内混段（seg1/seg3）；跨段依赖：T245→[T009:seg3, T157:seg4, T211:seg3, T213:seg2, T238:seg3, T341:seg4, T342:seg3, T353:seg4, T354:seg3, T404:seg4, T450:seg2, T463:seg3, T545:seg2, T583:seg2, T587:seg2]; T319→[T029:seg4, T070:seg2, T096:seg4, T213:seg2, T341:seg4, T645:seg2, T647:seg2]

### Wave 181 — T147, T578
- **任务/area**：T147(windows/workflow), T578(windows/connection)
- **当前 shard**：T147:seg2, T578:seg1
- **跨 shard 警告**：wave 内混段（seg1/seg2）；跨段依赖：T147→[T018:seg4, T038:seg4, T079:seg3, T088:seg3, T096:seg4, T112:seg4, T142:seg3, T165:seg3, T190:seg4, T237:seg4, T264:seg3, T319:seg3, T341:seg4, T365:seg4, T388:seg4, T404:seg4, T458:seg3, T488:seg3]; T578→[T029:seg4, T039:seg3, T084:seg4, T159:seg3, T190:seg4, T213:seg2, T258:seg4, T279:seg4, T340:seg2, T354:seg3, T356:seg4, T364:seg2, T394:seg2, T431:seg3, T450:seg2, T482:seg2, T507:seg2, T555:seg2, T573:seg2, T589:seg2, T599:seg2, T617:seg2, T631:seg2, T645:seg2]

### Wave 182 — T067
- **任务/area**：T067(windows/connection)
- **当前 shard**：T067:seg1
- **跨 shard 警告**：跨段依赖：T067→[T029:seg4, T038:seg4, T039:seg3, T127:seg3, T213:seg2, T341:seg4, T354:seg3, T366:seg3, T403:seg2, T431:seg3, T450:seg2, T458:seg3, T463:seg3, T513:seg2, T629:seg2, T640:seg2]

### Wave 183 — T590
- **任务/area**：T590(windows/connection)
- **当前 shard**：T590:seg1
- **跨 shard 警告**：跨段依赖：T590→[T091:seg2, T112:seg4, T213:seg2, T237:seg4, T243:seg3, T272:seg2, T276:seg3, T354:seg3, T377:seg3, T437:seg2, T617:seg2, T631:seg2, T645:seg2]

### Wave 184 — T520
- **任务/area**：T520(windows/connection)
- **当前 shard**：T520:seg1
- **跨 shard 警告**：跨段依赖：T520→[T038:seg4, T039:seg3, T088:seg3, T131:seg3, T155:seg4, T181:seg2, T207:seg2, T211:seg3, T233:seg4, T242:seg4, T243:seg3, T340:seg2, T341:seg4, T354:seg3, T387:seg3, T395:seg4, T404:seg4, T431:seg3, T446:seg3, T450:seg2, T458:seg3, T491:seg2, T577:seg2, T605:seg2, T611:seg2, T615:seg2]

### Wave 185 — T608
- **任务/area**：T608(windows/connection)
- **当前 shard**：T608:seg1
- **跨 shard 警告**：跨段依赖：T608→[T038:seg4, T084:seg4, T127:seg3, T341:seg4, T354:seg3, T356:seg4, T431:seg3, T450:seg2, T463:seg3, T491:seg2, T507:seg2, T567:seg2, T589:seg2, T599:seg2, T617:seg2, T631:seg2, T645:seg2]

### Wave 186 — T610
- **任务/area**：T610(windows/connection)
- **当前 shard**：T610:seg1
- **跨 shard 警告**：跨段依赖：T610→[T039:seg3, T190:seg4, T341:seg4, T437:seg2]

### Wave 187 — T612
- **任务/area**：T612(windows/connection)
- **当前 shard**：T612:seg1
- **跨 shard 警告**：跨段依赖：T612→[T039:seg3, T127:seg3, T213:seg2, T354:seg3, T431:seg3, T450:seg2, T491:seg2]

### Wave 188 — T614
- **任务/area**：T614(windows/connection)
- **当前 shard**：T614:seg1
- **跨 shard 警告**：跨段依赖：T614→[T041:seg3, T129:seg3, T378:seg3]

### Wave 189 — T616
- **任务/area**：T616(windows/connection)
- **当前 shard**：T616:seg1
- **跨 shard 警告**：跨段依赖：T616→[T038:seg4, T341:seg4, T458:seg3, T645:seg2]

### Wave 190 — T618
- **任务/area**：T618(windows/connection)
- **当前 shard**：T618:seg1
- **跨 shard 警告**：跨段依赖：T618→[T213:seg2, T354:seg3, T450:seg2, T545:seg2, T547:seg2]

### Wave 191 — T620
- **任务/area**：T620(windows/connection)
- **当前 shard**：T620:seg1
- **跨 shard 警告**：跨段依赖：T620→[T187:seg3, T213:seg2, T344:seg4, T565:seg2]

### Wave 192 — T622
- **任务/area**：T622(windows/connection)
- **当前 shard**：T622:seg1
- **跨 shard 警告**：跨段依赖：T622→[T029:seg4, T039:seg3, T213:seg2, T341:seg4, T354:seg3, T366:seg3, T421:seg2, T431:seg3, T450:seg2, T463:seg3, T477:seg2, T567:seg2, T575:seg2, T631:seg2]

### Wave 193 — T624
- **任务/area**：T624(windows/connection)
- **当前 shard**：T624:seg1
- **跨 shard 警告**：跨段依赖：T624→[T090:seg3, T212:seg3, T213:seg2, T296:seg3, T463:seg3, T472:seg2, T629:seg2]

### Wave 194 — T626
- **任务/area**：T626(windows/connection)
- **当前 shard**：T626:seg1
- **跨 shard 警告**：跨段依赖：T626→[T160:seg3, T213:seg2, T378:seg3, T398:seg3, T463:seg3]

### Wave 195 — T628
- **任务/area**：T628(windows/connection)
- **当前 shard**：T628:seg1
- **跨 shard 警告**：跨段依赖：T628→[T041:seg3, T212:seg3, T213:seg2, T234:seg3, T275:seg4, T354:seg3, T366:seg3, T422:seg4, T515:seg2, T517:seg2, T519:seg2, T525:seg2]

### Wave 196 — T630
- **任务/area**：T630(windows/connection)
- **当前 shard**：T630:seg1
- **跨 shard 警告**：跨段依赖：T630→[T212:seg3, T294:seg4, T378:seg3, T416:seg3, T525:seg2]

### Wave 197 — T554
- **任务/area**：T554(windows/connection)
- **当前 shard**：T554:seg1
- **跨 shard 警告**：跨段依赖：T554→[T038:seg4, T090:seg3, T127:seg3, T129:seg3, T157:seg4, T165:seg3, T210:seg4, T212:seg3, T213:seg2, T235:seg3, T257:seg2, T258:seg4, T277:seg3, T291:seg2, T298:seg4, T310:seg2, T315:seg3, T341:seg4, T355:seg3, T380:seg3, T416:seg3, T422:seg4, T450:seg2, T458:seg3, T463:seg3, T472:seg2, T513:seg2, T521:seg2, T525:seg2, T629:seg2, T640:seg2]

### Wave 198 — T485
- **任务/area**：T485(windows/connection)
- **当前 shard**：T485:seg1
- **跨 shard 警告**：跨段依赖：T485→[T127:seg3, T213:seg2, T341:seg4, T354:seg3, T421:seg2, T431:seg3, T450:seg2, T451:seg4, T491:seg2, T569:seg2, T589:seg2, T631:seg2]

### Wave 199 — T199
- **任务/area**：T199(windows/connection)
- **当前 shard**：T199:seg1
- **跨 shard 警告**：跨段依赖：T199→[T029:seg4, T127:seg3, T213:seg2, T230:seg2, T341:seg4, T354:seg3, T387:seg3, T412:seg2, T431:seg3, T450:seg2, T451:seg4, T458:seg3, T463:seg3, T541:seg2, T545:seg2, T599:seg2, T645:seg2]

### Wave 200 — T110
- **任务/area**：T110(windows/connection)
- **当前 shard**：T110:seg1
- **跨 shard 警告**：跨段依赖：T110→[T029:seg4, T038:seg4, T039:seg3, T073:seg2, T088:seg3, T096:seg4, T112:seg4, T127:seg3, T131:seg3, T147:seg2, T163:seg4, T186:seg3, T190:seg4, T197:seg3, T200:seg4, T284:seg3, T341:seg4, T354:seg3, T390:seg4, T404:seg4, T412:seg2, T437:seg2, T450:seg2, T458:seg3, T473:seg3, T488:seg3, T567:seg2, T645:seg2]

### Wave 201 — T020
- **任务/area**：T020(windows/main)
- **当前 shard**：T020:seg2
- **跨 shard 警告**：跨段依赖：T020→[T110:seg1, T127:seg3, T137:seg1, T167:seg1, T197:seg3, T239:seg1, T284:seg3, T341:seg4]

### Wave 202 — T051
- **任务/area**：T051(App.tsx)
- **当前 shard**：T051:seg3
- **跨 shard 警告**：跨段依赖：T051→[T020:seg2, T072:seg4, T309:seg4, T521:seg2, T603:seg2, T621:seg2, T644:seg2]

### Wave 203 — T047
- **任务/area**：T047(bootstrap)
- **当前 shard**：T047:seg4
- **跨 shard 警告**：跨段依赖：T047→[T046:seg2, T051:seg3, T162:seg3, T213:seg2, T234:seg3, T340:seg2, T354:seg3, T431:seg3, T458:seg3, T463:seg3, T472:seg2, T553:seg2, T589:seg2, T603:seg2, T621:seg2]

### 完整线性顺序（Rune-aware，便于机器读取）
```text
T014 T029 T033 T038 T042 T046 T049 T050 T053 T075 T077 T079 T080 T081 T091 T093 T031 T044 T095 T101
T105 T119 T120 T121 T082 T122 T123 T128 T130 T132 T133 T135 T058 T138 T142 T150 T136 T151 T154 T156
T083 T085 T159 T160 T161 T162 T163 T164 T166 T176 T185 T188 T189 T204 T209 T211 T212 T214 T227 T229
T233 T236 T237 T242 T063 T146 T246 T251 T213 T267 T268 T271 T202 T257 T272 T273 T274 T276 T278 T279
T048 T183 T232 T254 T208 T253 T283 T287 T231 T288 T293 T295 T157 T249 T252 T297 T086 T292 T298 T303
T131 T184 T307 T310 T021 T178 T311 T312 T125 T315 T316 T317 T037 T055 T140 T158 T323 T324 T327 T331
T116 T332 T335 T339 T196 T341 T356 T363 T290 T350 T353 T366 T205 T250 T365 T367 T018 T368 T370 T374
T379 T381 T385 T386 T180 T387 T388 T399 T109 T393 T396 T397 T071 T244 T402 T406 T304 T362 T407 T414
T265 T410 T415 T416 T023 T345 T417 T419 T269 T308 T423 T424 T338 T427 T429 T432 T179 T437 T440 T441
T186 T383 T413 T444 T024 T445 T450 T452 T035 T107 T124 T182 T030 T036 T210 T364 T065 T318 T392 T394
T373 T375 T451 T459 T190 T228 T457 T465 T306 T401 T468 T469 T358 T428 T472 T473 T461 T474 T475 T477
T434 T466 T482 T488 T471 T484 T491 T492 T013 T092 T096 T255 T059 T087 T097 T207 T266 T285 T455 T483
T022 T198 T224 T481 T073 T371 T486 T494 T493 T497 T501 T499 T505 T530 T289 T509 T532 T496 T511 T538
T502 T513 T542 T187 T216 T436 T515 T056 T234 T490 T521 T108 T523 T544 T500 T548 T043 T525 T564 T094
T275 T425 T456 T045 T326 T566 T527 T568 T313 T349 T531 T361 T533 T535 T562 T064 T537 T572 T134 T264
T539 T576 T284 T409 T541 T170 T435 T543 T041 T302 T545 T586 T256 T320 T546 T547 T263 T549 T592 T321
T553 T596 T555 T600 T557 T594 T418 T559 T376 T449 T561 T602 T074 T106 T117 T438 T115 T325 T565 T192
T248 T567 T155 T260 T512 T569 T084 T193 T571 T217 T352 T028 T194 T281 T573 T060 T103 T153 T577 T168
T300 T579 T104 T581 T019 T218 T585 T113 T240 T522 T587 T098 T261 T591 T025 T593 T206 T595 T076 T597
T599 T601 T528 T603 T462 T328 T605 T607 T359 T613 T347 T382 T615 T220 T426 T516 T609 T617 T340 T439
T582 T403 T405 T039 T611 T127 T570 T619 T181 T314 T333 T348 T487 T574 T575 T102 T598 T621 T589 T422
T503 T623 T627 T625 T629 T129 T258 T299 T529 T219 T235 T280 T286 T259 T277 T344 T351 T343 T372 T380
T411 T238 T330 T420 T431 T090 T329 T391 T398 T355 T400 T446 T467 T296 T442 T476 T507 T126 T378 T433
T448 T139 T165 T294 T463 T069 T377 T470 T551 T175 T354 T495 T583 T223 T342 T395 T517 T498 T519 T504
T631 T460 T632 T141 T221 T508 T633 T011 T222 T322 T430 T243 T404 T458 T514 T070 T201 T239 T305 T225
T337 T634 T421 T480 T346 T524 T635 T137 T526 T636 T247 T412 T534 T167 T177 T536 T637 T203 T447 T550
T638 T226 T453 T478 T560 T144 T464 T639 T489 T640 T291 T454 T040 T143 T230 T556 T061 T089 T584 T641
T017 T052 T088 T169 T015 T111 T148 T195 T145 T336 T588 T643 T174 T360 T518 T642 T068 T408 T604 T644
T012 T026 T200 T479 T066 T171 T563 T580 T149 T172 T384 T443 T390 T606 T645 T112 T114 T173 T197 T072
T241 T270 T309 T262 T558 T646 T282 T510 T647 T301 T506 T334 T540 T009 T552 T245 T319 T147 T578 T067
T590 T520 T608 T610 T612 T614 T616 T618 T620 T622 T624 T626 T628 T630 T554 T485 T199 T110 T020 T051
T047
```

## 2. Foundation pre-pass（应当最先迁的高扇入 L0/L1）

取自 §3.1「被更早任务依赖次数」表。Rune-aware 口径下，表中带 Rune 等价实现的任务可**标记 done-with-note 而不必重新迁**。

| 任务 | 可前移反向边 | area | shard | 文件 | 处置 |
| --- | ---: | --- | ---: | --- | --- |
| T341 | 133 | hooks | 4 | `hooks/useI18n.ts` | **已满足基座 → done-with-note**（hooks/useI18n.ts -> crate::i18n::t (i18n.rs)） |
| T213 | 69 | types | 2 | `types/index.ts` | **已满足基座 → done-with-note**（types/index.ts -> backend-client Rust mirror + local VMs (types.rs)） |
| T458 | 40 | stores | 3 | `stores/settingsStore.ts` | **已满足基座 → done-with-note**（stores/settingsStore.ts -> crate::settings_state slice） |
| T450 | 35 | lib | 2 | `lib/databaseTypes.ts` | 优先前移 |
| T629 | 27 | lib | 2 | `lib/tableChanges.ts` | 优先前移 |
| T645 | 22 | lib | 2 | `lib/windowManager.ts` | 优先前移 |
| T567 | 19 | lib | 2 | `lib/nativeContextMenu.ts` | 优先前移 |
| T617 | 19 | lib | 2 | `lib/sqlGenerator.ts` | 优先前移 |
| T589 | 18 | lib | 2 | `lib/schemaCache.ts` | 优先前移 |
| T437 | 15 | lib | 2 | `lib/crossWindowBus.ts` | 优先前移 |
| T406 | 14 | commands | 4 | `commands/sync.ts` | 优先前移 |
| T399 | 13 | windows/settings | 4 | `windows/settings/settingsUi.tsx` | 优先前移 |
| T525 | 12 | lib | 2 | `lib/gridErrors.ts` | 优先前移 |
| T419 | 11 | windows/data-sync | 3 | `windows/data-sync/mappingView.ts` | 优先前移 |
| T365 | 11 | hooks | 4 | `hooks/useLocaleDomains.ts` | 优先前移 |
| T502 | 10 | windows/data-sync | 3 | `windows/data-sync/utils.ts` | 优先前移 |
| T131 | 10 | platform | 3 | `platform/contracts.ts` | 优先前移 |
| T431 | 10 | stores | 3 | `stores/schemaStore.ts` | 优先前移 |
| T404 | 10 | hooks | 4 | `hooks/useResizable.ts` | 优先前移 |
| T379 | 10 | commands | 4 | `commands/schemaDiff.ts` | 优先前移 |
| T491 | 9 | lib | 2 | `lib/errors.ts` | 优先前移 |
| T488 | 9 | stores | 3 | `stores/workspacePanelStateStore.ts` | 优先前移 |
| T631 | 9 | lib | 2 | `lib/tableSchemaForSql.ts` | 优先前移 |
| T378 | 9 | stores/tableData | 3 | `stores/tableData/types.ts` | 优先前移 |
| T541 | 9 | lib | 2 | `lib/importMapping.ts` | 优先前移 |
| T096 | 8 | commands | 4 | `commands/ai.ts` | 优先前移 |
| T354 | 8 | stores | 3 | `stores/panelStore.ts` | 优先前移 |
| T352 | 8 | lib | 2 | `lib/cellEditorCandidates.ts` | 优先前移 |
| T356 | 7 | commands | 4 | `commands/query.ts` | 优先前移 |
| T333 | 7 | components/connection | 1 | `components/connection/useConnectionForm.ts` | 优先前移 |
| T091 | (types 总桶同族) | types | 2 | `types/chart.ts` | **已满足基座 → done-with-note**（types/chart.ts -> dashboard domain types / backend-client chart mirror） |

**已满足基座任务集合（共 4 个 remaining）**：T091（types/chart.ts -> dashboard domain types / backend-client chart mirror）, T213（types/index.ts -> backend-client Rust mirror + local VMs (types.rs)）, T341（hooks/useI18n.ts -> crate::i18n::t (i18n.rs)）, T458（stores/settingsStore.ts -> crate::settings_state slice）。
这些任务作为目标的所有边（value + type）在 Rune-aware 图中被删除；它们是文档 §6 所述 231 条“指向已完成任务”+ 347 条“指向非台账基座”命中在 remaining 任务集内的落点。其余基座（Button/Input/Checkbox/Dialog/Select/ErrorBanner/Spinner/CopyableError/cn.ts/tid.ts/useCopyFeedback）与 `support/{formatters,cell_types,cell_editor_kind,data_type_colors,to_edit_string}` 均不在 remaining 台账内（已完成或非台账），不产生边。

## 3. 三顺序对比（反向依赖消除量，分母 1406）

| 顺序 | 消除的反向边 | 说明 |
| --- | ---: | --- |
| 当前 wave / T 编号顺序 | 0 | 按定义 B 边均为 from<to，编号顺序全部未满足 |
| Literal 拓扑（降 type，保留全部 value 边） | 1299 | 全部 1133 条 value 反向边按拓扑被满足；type 反向边部分偶然满足 |
| Rune-aware 拓扑（再删已满足基座边） | 1308 | 在 Literal 基础上，指向 T091, T213, T341, T458 的反向边视为基座已存在而满足 |

细分：原始 1406 条反向边中 value 1133、type 273；Literal 消除 1299（value 全消 1133），Rune-aware 消除 1308。
Literal 图中删除的 type 边 606 条（仅降级、不计硬排序约束）；Rune-aware 图额外删除已满足基座边 538 条（其中 value 302）。

## 4. 环与处理

- Literal 硬约束图（仅 value 边）多节点 SCC：7，最大 4 节点。
- Rune-aware 硬约束图多节点 SCC：7，最大 4 节点：T037 ↔ T125 ↔ T184, T039 ↔ T311 ↔ T405 ↔ T477, T082 ↔ T121, T131 ↔ T298, T280 ↔ T299, T347 ↔ T359, T377 ↔ T577。
- 53 节点的“假环”由 `types/index.ts` 等总桶的 type-only/re-export 造成，降级 type 边后消失；剩余真实环最大 4 节点（如 T039↔T311↔T405↔T477）。处理方式：Tarjan 找出 SCC 后丢弃环内“高 T→低 T”的边打破环，环内按 T 编号线性化，迁移时“先定接口再实现”即可解开，不构成全局阻塞。

## 5. 范围声明

**本文件不修改 `tasklist.md` 或 `shards/*.md`，也不改动任何产品源码/脚本。** 它只是交给协调者审阅的**提案顺序**；在协调者决定切换前，旧台账与 shard 文件仍是唯一事实来源（source of truth）。

