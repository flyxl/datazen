SELECT nested FROM t WHERE id IN (0, 1, 2);
SELECT nested FROM t WHERE id IN (1, 2, 3);
SELECT `mysql_2` FROM `tbl_2`;
SELECT * FROM "quoted_3" WHERE col = E'esc\'3';
SELECT * FROM "quoted_4" WHERE col = E'esc\'4';
INSERT INTO bench_t_5 (id, payload) VALUES (5, 'v5');
SELECT 6 AS id, 'row_6' AS label;
SELECT nested FROM t WHERE id IN (7, 8, 9);
-- line 8: deterministic comment
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
INSERT INTO bench_t_10 (id, payload) VALUES (10, 'v10');
WITH cte_11 AS (SELECT 11 AS n) SELECT n FROM cte_11;
$dz$ dollar body 12 ; semicolon inside $dz$
SELECT 13 AS id, 'row_13' AS label;
DELETE FROM bench_t_14 WHERE id = 14;
/* block header 15 */
SELECT 16 AS id, 'row_16' AS label;
WITH cte_17 AS (SELECT 17 AS n) SELECT n FROM cte_17;
SELECT `mysql_18` FROM `tbl_18`;
BEGIN; SELECT 19; COMMIT;
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT * FROM "quoted_21" WHERE col = E'esc\'21';
INSERT INTO bench_t_22 (id, payload) VALUES (22, 'O''Brien');
# hash comment 23
BEGIN; SELECT 24; COMMIT;
/* block header 25 */
DELETE FROM bench_t_26 WHERE id = 10;
WITH cte_27 AS (SELECT 27 AS n) SELECT n FROM cte_27;
WITH cte_28 AS (SELECT 28 AS n) SELECT n FROM cte_28;
SELECT * FROM "quoted_29" WHERE col = E'esc\'29';
SELECT 30 AS id, 'row_30' AS label;
DELETE FROM bench_t_31 WHERE id = 15;
/* block header 32 */
WITH cte_33 AS (SELECT 33 AS n) SELECT n FROM cte_33;
INSERT INTO bench_t_34 (id, payload) VALUES (34, 'v34');
/* block header 35 */
-- line 36: deterministic comment
SELECT 37 AS id, 'row_37' AS label;
SELECT * FROM "quoted_38" WHERE col = E'esc\'38';
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
/* block header 40 */
SELECT 41 AS id, 'row_41' AS label;
SELECT nested FROM t WHERE id IN (42, 43, 44);
# hash comment 43
-- line 44: deterministic comment
WITH cte_45 AS (SELECT 45 AS n) SELECT n FROM cte_45;
BEGIN; SELECT 46; COMMIT;
# hash comment 47
$dz$ dollar body 48 ; semicolon inside $dz$
$dz$ dollar body 49 ; semicolon inside $dz$
SELECT 50 AS id, 'row_50' AS label;
WITH cte_51 AS (SELECT 51 AS n) SELECT n FROM cte_51;
WITH cte_52 AS (SELECT 52 AS n) SELECT n FROM cte_52;
UPDATE bench_t_53 SET payload = 53 WHERE id = 21;
UPDATE bench_t_54 SET payload = 54 WHERE id = 22;
INSERT INTO bench_t_55 (id, payload) VALUES (55, 'O''Brien');
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
$dz$ dollar body 57 ; semicolon inside $dz$
SELECT nested FROM t WHERE id IN (58, 59, 60);
$dz$ dollar body 59 ; semicolon inside $dz$
BEGIN; SELECT 60; COMMIT;
SELECT `mysql_61` FROM `tbl_11`;
INSERT INTO bench_t_62 (id, payload) VALUES (62, 'v62');
SELECT `mysql_63` FROM `tbl_13`;
BEGIN; SELECT 64; COMMIT;
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
UPDATE bench_t_2 SET payload = 66 WHERE id = 2;
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
BEGIN; SELECT 68; COMMIT;
SELECT `mysql_69` FROM `tbl_19`;
SELECT * FROM "quoted_70" WHERE col = E'esc\'70';
SELECT 71 AS id, 'row_71' AS label;
SELECT nested FROM t WHERE id IN (72, 73, 74);
SELECT nested FROM t WHERE id IN (73, 74, 75);
SELECT `mysql_74` FROM `tbl_24`;
SELECT `mysql_75` FROM `tbl_25`;
SELECT 76 AS id, 'row_76' AS label;
WITH cte_77 AS (SELECT 77 AS n) SELECT n FROM cte_77;
BEGIN; SELECT 78; COMMIT;
SELECT [bracket_79] FROM [dbo].[tbl_39];
$dz$ dollar body 80 ; semicolon inside $dz$
-- line 81: deterministic comment
-- line 82: deterministic comment
/* block header 83 */
SELECT nested FROM t WHERE id IN (84, 85, 86);
INSERT INTO bench_t_85 (id, payload) VALUES (85, 'v85');
/* block header 86 */
INSERT INTO bench_t_87 (id, payload) VALUES (87, 'v87');
WITH cte_88 AS (SELECT 88 AS n) SELECT n FROM cte_88;
BEGIN; SELECT 89; COMMIT;
-- line 90: deterministic comment
SELECT 91 AS id, 'row_91' AS label;
$dz$ dollar body 92 ; semicolon inside $dz$
INSERT INTO bench_t_93 (id, payload) VALUES (93, 'v93');
SELECT nested FROM t WHERE id IN (94, 95, 96);
WITH cte_95 AS (SELECT 95 AS n) SELECT n FROM cte_95;
INSERT INTO bench_t_96 (id, payload) VALUES (96, 'v96');
WITH cte_97 AS (SELECT 97 AS n) SELECT n FROM cte_97;
INSERT INTO bench_t_98 (id, payload) VALUES (98, 'v98');
$dz$ dollar body 99 ; semicolon inside $dz$
$dz$ dollar body 100 ; semicolon inside $dz$
# hash comment 101
-- line 102: deterministic comment
DELETE FROM bench_t_7 WHERE id = 7;
DELETE FROM bench_t_8 WHERE id = 8;
$dz$ dollar body 105 ; semicolon inside $dz$
SELECT nested FROM t WHERE id IN (106, 107, 108);
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
-- line 108: deterministic comment
$dz$ dollar body 109 ; semicolon inside $dz$
$dz$ dollar body 110 ; semicolon inside $dz$
SELECT * FROM "quoted_111" WHERE col = E'esc\'111';
SELECT * FROM "quoted_112" WHERE col = E'esc\'112';
BEGIN; SELECT 113; COMMIT;
UPDATE bench_t_50 SET payload = 114 WHERE id = 18;
SELECT 115 AS id, 'row_115' AS label;
INSERT INTO bench_t_116 (id, payload) VALUES (116, 'v116');
SELECT [bracket_117] FROM [dbo].[tbl_37];
SELECT * FROM "quoted_118" WHERE col = E'esc\'118';
/* block header 119 */
DELETE FROM bench_t_24 WHERE id = 8;
INSERT INTO bench_t_121 (id, payload) VALUES (121, 'O''Brien');
SELECT 122 AS id, 'row_122' AS label;
# hash comment 123
SELECT [bracket_124] FROM [dbo].[tbl_4];
INSERT INTO bench_t_125 (id, payload) VALUES (125, 'v125');
SELECT * FROM "quoted_126" WHERE col = E'esc\'126';
DELETE FROM bench_t_31 WHERE id = 15;
SELECT 128 AS id, 'row_128' AS label;
UPDATE bench_t_1 SET payload = 129 WHERE id = 1;
INSERT INTO bench_t_2 (id, payload) VALUES (130, 'v130');
UPDATE bench_t_3 SET payload = 131 WHERE id = 3;
SELECT nested FROM t WHERE id IN (132, 133, 134);
BEGIN; SELECT 133; COMMIT;
SELECT 134 AS id, 'row_134' AS label;
SELECT `mysql_135` FROM `tbl_35`;
INSERT INTO bench_t_8 (id, payload) VALUES (136, 'v136');
SELECT `mysql_137` FROM `tbl_37`;
# hash comment 138
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
# hash comment 140
SELECT nested FROM t WHERE id IN (141, 142, 143);
$dz$ dollar body 142 ; semicolon inside $dz$
$dz$ dollar body 143 ; semicolon inside $dz$
DELETE FROM bench_t_16 WHERE id = 0;
$dz$ dollar body 145 ; semicolon inside $dz$
$dz$ dollar body 146 ; semicolon inside $dz$
SELECT [bracket_147] FROM [dbo].[tbl_27];
BEGIN; SELECT 148; COMMIT;
-- line 149: deterministic comment
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
-- line 151: deterministic comment
# hash comment 152
UPDATE bench_t_25 SET payload = 153 WHERE id = 25;
SELECT nested FROM t WHERE id IN (154, 155, 156);
SELECT [bracket_155] FROM [dbo].[tbl_35];
UPDATE bench_t_28 SET payload = 156 WHERE id = 28;
SELECT * FROM "quoted_157" WHERE col = E'esc\'157';
UPDATE bench_t_30 SET payload = 158 WHERE id = 30;
-- line 159: deterministic comment
SELECT [bracket_160] FROM [dbo].[tbl_0];
SELECT [bracket_161] FROM [dbo].[tbl_1];
-- line 162: deterministic comment
WITH cte_163 AS (SELECT 163 AS n) SELECT n FROM cte_163;
SELECT nested FROM t WHERE id IN (164, 165, 166);
-- line 165: deterministic comment
SELECT `mysql_166` FROM `tbl_16`;
SELECT 167 AS id, 'row_167' AS label;
SELECT `mysql_168` FROM `tbl_18`;
SELECT * FROM "quoted_169" WHERE col = E'esc\'169';
SELECT * FROM "quoted_170" WHERE col = E'esc\'170';
/* block header 171 */
SELECT 172 AS id, 'row_172' AS label;
SELECT nested FROM t WHERE id IN (173, 174, 175);
SELECT nested FROM t WHERE id IN (174, 175, 176);
SELECT * FROM "quoted_175" WHERE col = E'esc\'175';
$dz$ dollar body 176 ; semicolon inside $dz$
UPDATE bench_t_49 SET payload = 177 WHERE id = 17;
SELECT * FROM "quoted_178" WHERE col = E'esc\'178';
# hash comment 179
SELECT `mysql_180` FROM `tbl_30`;
BEGIN; SELECT 181; COMMIT;
SELECT `mysql_182` FROM `tbl_32`;
SELECT `mysql_183` FROM `tbl_33`;
# hash comment 184
SELECT [bracket_185] FROM [dbo].[tbl_25];
UPDATE bench_t_58 SET payload = 186 WHERE id = 26;
WITH cte_187 AS (SELECT 187 AS n) SELECT n FROM cte_187;
INSERT INTO bench_t_60 (id, payload) VALUES (188, 'v188');
UPDATE bench_t_61 SET payload = 189 WHERE id = 29;
$dz$ dollar body 190 ; semicolon inside $dz$
/* block header 191 */
# hash comment 192
# hash comment 193
-- line 194: deterministic comment
SELECT 195 AS id, 'row_195' AS label;
SELECT nested FROM t WHERE id IN (196, 197, 198);
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
WITH cte_198 AS (SELECT 198 AS n) SELECT n FROM cte_198;
UPDATE bench_t_7 SET payload = 199 WHERE id = 7;
BEGIN; SELECT 200; COMMIT;
BEGIN; SELECT 201; COMMIT;
# hash comment 202
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT * FROM "quoted_204" WHERE col = E'esc\'204';
SELECT 205 AS id, 'row_205' AS label;
-- line 206: deterministic comment
INSERT INTO bench_t_79 (id, payload) VALUES (207, 'v207');
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT [bracket_209] FROM [dbo].[tbl_9];
WITH cte_210 AS (SELECT 210 AS n) SELECT n FROM cte_210;
$dz$ dollar body 211 ; semicolon inside $dz$
$dz$ dollar body 212 ; semicolon inside $dz$
BEGIN; SELECT 213; COMMIT;
SELECT 214 AS id, 'row_214' AS label;
WITH cte_215 AS (SELECT 215 AS n) SELECT n FROM cte_215;
SELECT `mysql_216` FROM `tbl_16`;
INSERT INTO bench_t_89 (id, payload) VALUES (217, 'v217');
INSERT INTO bench_t_90 (id, payload) VALUES (218, 'v218');
WITH cte_219 AS (SELECT 219 AS n) SELECT n FROM cte_219;
SELECT * FROM "quoted_220" WHERE col = E'esc\'220';
SELECT `mysql_221` FROM `tbl_21`;
WITH cte_222 AS (SELECT 222 AS n) SELECT n FROM cte_222;
SELECT `mysql_223` FROM `tbl_23`;
# hash comment 224
SELECT `mysql_225` FROM `tbl_25`;
DELETE FROM bench_t_2 WHERE id = 2;
SELECT 227 AS id, 'row_227' AS label;
SELECT [bracket_228] FROM [dbo].[tbl_28];
DELETE FROM bench_t_5 WHERE id = 5;
SELECT * FROM "quoted_230" WHERE col = E'esc\'230';
# hash comment 231
WITH cte_232 AS (SELECT 232 AS n) SELECT n FROM cte_232;
DELETE FROM bench_t_9 WHERE id = 9;
INSERT INTO bench_t_106 (id, payload) VALUES (234, 'v234');
BEGIN; SELECT 235; COMMIT;
BEGIN; SELECT 236; COMMIT;
# hash comment 237
WITH cte_238 AS (SELECT 238 AS n) SELECT n FROM cte_238;
UPDATE bench_t_47 SET payload = 239 WHERE id = 15;
BEGIN; SELECT 240; COMMIT;
UPDATE bench_t_49 SET payload = 241 WHERE id = 17;
BEGIN; SELECT 242; COMMIT;
BEGIN; SELECT 243; COMMIT;
UPDATE bench_t_52 SET payload = 244 WHERE id = 20;
/* block header 245 */
WITH cte_246 AS (SELECT 246 AS n) SELECT n FROM cte_246;
DELETE FROM bench_t_23 WHERE id = 7;
SELECT `mysql_248` FROM `tbl_48`;
UPDATE bench_t_57 SET payload = 249 WHERE id = 25;
/*
 * section 1
 * checksum 602f
 */
WITH cte_250 AS (SELECT 250 AS n) SELECT n FROM cte_250;
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
$dz$ dollar body 256 ; semicolon inside $dz$
UPDATE bench_t_1 SET payload = 257 WHERE id = 1;
$dz$ dollar body 258 ; semicolon inside $dz$
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
/* block header 260 */
WITH cte_261 AS (SELECT 261 AS n) SELECT n FROM cte_261;
$dz$ dollar body 262 ; semicolon inside $dz$
# hash comment 263
SELECT `mysql_264` FROM `tbl_14`;
BEGIN; SELECT 265; COMMIT;
# hash comment 266
SELECT * FROM "quoted_267" WHERE col = E'esc\'267';
WITH cte_268 AS (SELECT 268 AS n) SELECT n FROM cte_268;
/* block header 269 */
SELECT 270 AS id, 'row_270' AS label;
WITH cte_271 AS (SELECT 271 AS n) SELECT n FROM cte_271;
DELETE FROM bench_t_16 WHERE id = 0;
INSERT INTO bench_t_17 (id, payload) VALUES (273, 'v273');
INSERT INTO bench_t_18 (id, payload) VALUES (274, 'v274');
$dz$ dollar body 275 ; semicolon inside $dz$
SELECT `mysql_276` FROM `tbl_26`;
$dz$ dollar body 277 ; semicolon inside $dz$
SELECT 278 AS id, 'row_278' AS label;
# hash comment 279
INSERT INTO bench_t_24 (id, payload) VALUES (280, 'v280');
SELECT `mysql_281` FROM `tbl_31`;
# hash comment 282
SELECT nested FROM t WHERE id IN (283, 284, 285);
INSERT INTO bench_t_28 (id, payload) VALUES (284, 'v284');
/* block header 285 */
DELETE FROM bench_t_30 WHERE id = 14;
SELECT nested FROM t WHERE id IN (287, 288, 289);
INSERT INTO bench_t_32 (id, payload) VALUES (288, 'v288');
SELECT [bracket_289] FROM [dbo].[tbl_9];
DELETE FROM bench_t_2 WHERE id = 2;
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
WITH cte_292 AS (SELECT 292 AS n) SELECT n FROM cte_292;
SELECT * FROM "quoted_293" WHERE col = E'esc\'293';
BEGIN; SELECT 294; COMMIT;
WITH cte_295 AS (SELECT 295 AS n) SELECT n FROM cte_295;
SELECT 296 AS id, 'row_296' AS label;
WITH cte_297 AS (SELECT 297 AS n) SELECT n FROM cte_297;
DELETE FROM bench_t_10 WHERE id = 10;
UPDATE bench_t_43 SET payload = 299 WHERE id = 11;
WITH cte_300 AS (SELECT 300 AS n) SELECT n FROM cte_300;
/* block header 301 */
DELETE FROM bench_t_14 WHERE id = 14;
SELECT `mysql_303` FROM `tbl_3`;
BEGIN; SELECT 304; COMMIT;
/* block header 305 */
$dz$ dollar body 306 ; semicolon inside $dz$
/* block header 307 */
INSERT INTO bench_t_52 (id, payload) VALUES (308, 'O''Brien');
$dz$ dollar body 309 ; semicolon inside $dz$
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
WITH cte_311 AS (SELECT 311 AS n) SELECT n FROM cte_311;
/* block header 312 */
SELECT `mysql_313` FROM `tbl_13`;
SELECT 314 AS id, 'row_314' AS label;
SELECT [bracket_315] FROM [dbo].[tbl_35];
DELETE FROM bench_t_28 WHERE id = 12;
# hash comment 317
# hash comment 318
INSERT INTO bench_t_63 (id, payload) VALUES (319, 'O''Brien');
WITH cte_320 AS (SELECT 320 AS n) SELECT n FROM cte_320;
SELECT 321 AS id, 'row_321' AS label;
$dz$ dollar body 322 ; semicolon inside $dz$
SELECT nested FROM t WHERE id IN (323, 324, 325);
/* block header 324 */
SELECT `mysql_325` FROM `tbl_25`;
SELECT [bracket_326] FROM [dbo].[tbl_6];
WITH cte_327 AS (SELECT 327 AS n) SELECT n FROM cte_327;
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT nested FROM t WHERE id IN (329, 330, 331);
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
-- line 331: deterministic comment
$dz$ dollar body 332 ; semicolon inside $dz$
SELECT nested FROM t WHERE id IN (333, 334, 335);
$dz$ dollar body 334 ; semicolon inside $dz$
SELECT nested FROM t WHERE id IN (335, 336, 337);
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT `mysql_337` FROM `tbl_37`;
SELECT 338 AS id, 'row_338' AS label;
-- line 339: deterministic comment
BEGIN; SELECT 340; COMMIT;
DELETE FROM bench_t_21 WHERE id = 5;
BEGIN; SELECT 342; COMMIT;
WITH cte_343 AS (SELECT 343 AS n) SELECT n FROM cte_343;
SELECT * FROM "quoted_344" WHERE col = E'esc\'344';
SELECT [bracket_345] FROM [dbo].[tbl_25];
SELECT `mysql_346` FROM `tbl_46`;
$dz$ dollar body 347 ; semicolon inside $dz$
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
DELETE FROM bench_t_29 WHERE id = 13;
WITH cte_350 AS (SELECT 350 AS n) SELECT n FROM cte_350;
SELECT nested FROM t WHERE id IN (351, 352, 353);
SELECT `mysql_352` FROM `tbl_2`;
SELECT nested FROM t WHERE id IN (353, 354, 355);
UPDATE bench_t_34 SET payload = 354 WHERE id = 2;
SELECT * FROM "quoted_355" WHERE col = E'esc\'355';
WITH cte_356 AS (SELECT 356 AS n) SELECT n FROM cte_356;
SELECT `mysql_357` FROM `tbl_7`;
SELECT `mysql_358` FROM `tbl_8`;
SELECT * FROM "quoted_359" WHERE col = E'esc\'359';
SELECT `mysql_360` FROM `tbl_10`;
SELECT [bracket_361] FROM [dbo].[tbl_1];
-- line 362: deterministic comment
# hash comment 363
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
UPDATE bench_t_45 SET payload = 365 WHERE id = 13;
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
/* block header 367 */
BEGIN; SELECT 368; COMMIT;
WITH cte_369 AS (SELECT 369 AS n) SELECT n FROM cte_369;
DELETE FROM bench_t_18 WHERE id = 2;
SELECT `mysql_371` FROM `tbl_21`;
DELETE FROM bench_t_20 WHERE id = 4;
INSERT INTO bench_t_117 (id, payload) VALUES (373, 'v373');
BEGIN; SELECT 374; COMMIT;
SELECT nested FROM t WHERE id IN (375, 376, 377);
-- line 376: deterministic comment
SELECT [bracket_377] FROM [dbo].[tbl_17];
/* block header 378 */
SELECT nested FROM t WHERE id IN (379, 380, 381);
/* block header 380 */
DELETE FROM bench_t_29 WHERE id = 13;
DELETE FROM bench_t_30 WHERE id = 14;
BEGIN; SELECT 383; COMMIT;
-- line 384: deterministic comment
/* block header 385 */
SELECT * FROM "quoted_386" WHERE col = E'esc\'386';
SELECT `mysql_387` FROM `tbl_37`;
SELECT nested FROM t WHERE id IN (388, 389, 390);
# hash comment 389
# hash comment 390
SELECT nested FROM t WHERE id IN (391, 392, 393);
$dz$ dollar body 392 ; semicolon inside $dz$
WITH cte_393 AS (SELECT 393 AS n) SELECT n FROM cte_393;
# hash comment 394
SELECT `mysql_395` FROM `tbl_45`;
# hash comment 396
DELETE FROM bench_t_13 WHERE id = 13;
DELETE FROM bench_t_14 WHERE id = 14;
UPDATE bench_t_15 SET payload = 399 WHERE id = 15;
WITH cte_400 AS (SELECT 400 AS n) SELECT n FROM cte_400;
SELECT `mysql_401` FROM `tbl_1`;
/* block header 402 */
SELECT 403 AS id, 'row_403' AS label;
SELECT 404 AS id, 'row_404' AS label;
BEGIN; SELECT 405; COMMIT;
SELECT * FROM "quoted_406" WHERE col = E'esc\'406';
$dz$ dollar body 407 ; semicolon inside $dz$
DELETE FROM bench_t_24 WHERE id = 8;
SELECT 409 AS id, 'row_409' AS label;
WITH cte_410 AS (SELECT 410 AS n) SELECT n FROM cte_410;
SELECT `mysql_411` FROM `tbl_11`;
# hash comment 412
WITH cte_413 AS (SELECT 413 AS n) SELECT n FROM cte_413;
/* block header 414 */
BEGIN; SELECT 415; COMMIT;
SELECT [bracket_416] FROM [dbo].[tbl_16];
WITH cte_417 AS (SELECT 417 AS n) SELECT n FROM cte_417;
SELECT 418 AS id, 'row_418' AS label;
UPDATE bench_t_35 SET payload = 419 WHERE id = 3;
UPDATE bench_t_36 SET payload = 420 WHERE id = 4;
UPDATE bench_t_37 SET payload = 421 WHERE id = 5;
INSERT INTO bench_t_38 (id, payload) VALUES (422, 'v422');
UPDATE bench_t_39 SET payload = 423 WHERE id = 7;
/* block header 424 */
UPDATE bench_t_41 SET payload = 425 WHERE id = 9;
DELETE FROM bench_t_10 WHERE id = 10;
INSERT INTO bench_t_43 (id, payload) VALUES (427, 'v427');
UPDATE bench_t_44 SET payload = 428 WHERE id = 12;
/* block header 429 */
SELECT nested FROM t WHERE id IN (430, 431, 432);
INSERT INTO bench_t_47 (id, payload) VALUES (431, 'v431');
BEGIN; SELECT 432; COMMIT;
-- line 433: deterministic comment
SELECT [bracket_434] FROM [dbo].[tbl_34];
UPDATE bench_t_51 SET payload = 435 WHERE id = 19;
INSERT INTO bench_t_52 (id, payload) VALUES (436, 'v436');
-- line 437: deterministic comment
UPDATE bench_t_54 SET payload = 438 WHERE id = 22;
SELECT `mysql_439` FROM `tbl_39`;
INSERT INTO bench_t_56 (id, payload) VALUES (440, 'O''Brien');
# hash comment 441
SELECT * FROM "quoted_442" WHERE col = E'esc\'442';
UPDATE bench_t_59 SET payload = 443 WHERE id = 27;
-- line 444: deterministic comment
INSERT INTO bench_t_61 (id, payload) VALUES (445, 'v445');
SELECT 446 AS id, 'row_446' AS label;
SELECT * FROM "quoted_447" WHERE col = E'esc\'447';
DELETE FROM bench_t_0 WHERE id = 0;
SELECT [bracket_449] FROM [dbo].[tbl_9];
SELECT * FROM "quoted_450" WHERE col = E'esc\'450';
BEGIN; SELECT 451; COMMIT;
# hash comment 452
SELECT `mysql_453` FROM `tbl_3`;
SELECT [bracket_454] FROM [dbo].[tbl_14];
SELECT 455 AS id, 'row_455' AS label;
DELETE FROM bench_t_8 WHERE id = 8;
BEGIN; SELECT 457; COMMIT;
UPDATE bench_t_10 SET payload = 458 WHERE id = 10;
# hash comment 459
SELECT `mysql_460` FROM `tbl_10`;
BEGIN; SELECT 461; COMMIT;
$dz$ dollar body 462 ; semicolon inside $dz$
SELECT [bracket_463] FROM [dbo].[tbl_23];
# hash comment 464
SELECT nested FROM t WHERE id IN (465, 466, 467);
DELETE FROM bench_t_18 WHERE id = 2;
SELECT `mysql_467` FROM `tbl_17`;
BEGIN; SELECT 468; COMMIT;
$dz$ dollar body 469 ; semicolon inside $dz$
BEGIN; SELECT 470; COMMIT;
UPDATE bench_t_23 SET payload = 471 WHERE id = 23;
-- line 472: deterministic comment
/* block header 473 */
-- line 474: deterministic comment
SELECT [bracket_475] FROM [dbo].[tbl_35];
SELECT [bracket_476] FROM [dbo].[tbl_36];
INSERT INTO bench_t_93 (id, payload) VALUES (477, 'v477');
INSERT INTO bench_t_94 (id, payload) VALUES (478, 'v478');
WITH cte_479 AS (SELECT 479 AS n) SELECT n FROM cte_479;
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
DELETE FROM bench_t_1 WHERE id = 1;
# hash comment 482
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT `mysql_484` FROM `tbl_34`;
$dz$ dollar body 485 ; semicolon inside $dz$
BEGIN; SELECT 486; COMMIT;
BEGIN; SELECT 487; COMMIT;
-- line 488: deterministic comment
SELECT `mysql_489` FROM `tbl_39`;
BEGIN; SELECT 490; COMMIT;
$dz$ dollar body 491 ; semicolon inside $dz$
UPDATE bench_t_44 SET payload = 492 WHERE id = 12;
UPDATE bench_t_45 SET payload = 493 WHERE id = 13;
WITH cte_494 AS (SELECT 494 AS n) SELECT n FROM cte_494;
$dz$ dollar body 495 ; semicolon inside $dz$
/* block header 496 */
BEGIN; SELECT 497; COMMIT;
$dz$ dollar body 498 ; semicolon inside $dz$
INSERT INTO bench_t_115 (id, payload) VALUES (499, 'v499');
/*
 * section 2
 * checksum d4c7
 */
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
BEGIN; SELECT 505; COMMIT;
INSERT INTO bench_t_122 (id, payload) VALUES (506, 'O''Brien');
SELECT * FROM "quoted_507" WHERE col = E'esc\'507';
SELECT `mysql_508` FROM `tbl_8`;
SELECT nested FROM t WHERE id IN (509, 510, 511);
-- line 510: deterministic comment
SELECT nested FROM t WHERE id IN (511, 512, 513);
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
UPDATE bench_t_1 SET payload = 513 WHERE id = 1;
SELECT [bracket_514] FROM [dbo].[tbl_34];
SELECT nested FROM t WHERE id IN (515, 516, 517);
SELECT nested FROM t WHERE id IN (516, 517, 518);
SELECT * FROM "quoted_517" WHERE col = E'esc\'517';
BEGIN; SELECT 518; COMMIT;
SELECT [bracket_519] FROM [dbo].[tbl_39];
SELECT `mysql_520` FROM `tbl_20`;
SELECT 521 AS id, 'row_521' AS label;
-- line 522: deterministic comment
DELETE FROM bench_t_11 WHERE id = 11;
SELECT * FROM "quoted_524" WHERE col = E'esc\'524';
DELETE FROM bench_t_13 WHERE id = 13;
DELETE FROM bench_t_14 WHERE id = 14;
UPDATE bench_t_15 SET payload = 527 WHERE id = 15;
$dz$ dollar body 528 ; semicolon inside $dz$
-- line 529: deterministic comment
$dz$ dollar body 530 ; semicolon inside $dz$
BEGIN; SELECT 531; COMMIT;
WITH cte_532 AS (SELECT 532 AS n) SELECT n FROM cte_532;
SELECT [bracket_533] FROM [dbo].[tbl_13];
BEGIN; SELECT 534; COMMIT;
/* block header 535 */
BEGIN; SELECT 536; COMMIT;
INSERT INTO bench_t_25 (id, payload) VALUES (537, 'v537');
WITH cte_538 AS (SELECT 538 AS n) SELECT n FROM cte_538;
WITH cte_539 AS (SELECT 539 AS n) SELECT n FROM cte_539;
SELECT 540 AS id, 'row_540' AS label;
-- line 541: deterministic comment
-- line 542: deterministic comment
INSERT INTO bench_t_31 (id, payload) VALUES (543, 'v543');
INSERT INTO bench_t_32 (id, payload) VALUES (544, 'v544');
SELECT nested FROM t WHERE id IN (545, 546, 547);
-- line 546: deterministic comment
SELECT nested FROM t WHERE id IN (547, 548, 549);
SELECT `mysql_548` FROM `tbl_48`;
# hash comment 549
# hash comment 550
SELECT [bracket_551] FROM [dbo].[tbl_31];
SELECT [bracket_552] FROM [dbo].[tbl_32];
# hash comment 553
SELECT * FROM "quoted_554" WHERE col = E'esc\'554';
SELECT 555 AS id, 'row_555' AS label;
/* block header 556 */
BEGIN; SELECT 557; COMMIT;
UPDATE bench_t_46 SET payload = 558 WHERE id = 14;
# hash comment 559
$dz$ dollar body 560 ; semicolon inside $dz$
DELETE FROM bench_t_17 WHERE id = 1;
UPDATE bench_t_50 SET payload = 562 WHERE id = 18;
-- line 563: deterministic comment
# hash comment 564
DELETE FROM bench_t_21 WHERE id = 5;
/* block header 566 */
BEGIN; SELECT 567; COMMIT;
/* block header 568 */
/* block header 569 */
INSERT INTO bench_t_58 (id, payload) VALUES (570, 'v570');
SELECT [bracket_571] FROM [dbo].[tbl_11];
-- line 572: deterministic comment
BEGIN; SELECT 573; COMMIT;
SELECT * FROM "quoted_574" WHERE col = E'esc\'574';
SELECT [bracket_575] FROM [dbo].[tbl_15];
INSERT INTO bench_t_64 (id, payload) VALUES (576, 'v576');
SELECT `mysql_577` FROM `tbl_27`;
SELECT * FROM "quoted_578" WHERE col = E'esc\'578';
SELECT nested FROM t WHERE id IN (579, 580, 581);
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
DELETE FROM bench_t_5 WHERE id = 5;
SELECT [bracket_582] FROM [dbo].[tbl_22];
SELECT 583 AS id, 'row_583' AS label;
-- line 584: deterministic comment
INSERT INTO bench_t_73 (id, payload) VALUES (585, 'v585');
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
DELETE FROM bench_t_11 WHERE id = 11;
SELECT 588 AS id, 'row_588' AS label;
# hash comment 589
SELECT * FROM "quoted_590" WHERE col = E'esc\'590';
BEGIN; SELECT 591; COMMIT;
INSERT INTO bench_t_80 (id, payload) VALUES (592, 'v592');
DELETE FROM bench_t_17 WHERE id = 1;
SELECT `mysql_594` FROM `tbl_44`;
UPDATE bench_t_19 SET payload = 595 WHERE id = 19;
DELETE FROM bench_t_20 WHERE id = 4;
/* block header 597 */
SELECT * FROM "quoted_598" WHERE col = E'esc\'598';
UPDATE bench_t_23 SET payload = 599 WHERE id = 23;
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
DELETE FROM bench_t_25 WHERE id = 9;
# hash comment 602
-- line 603: deterministic comment
SELECT * FROM "quoted_604" WHERE col = E'esc\'604';
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT `mysql_606` FROM `tbl_6`;
SELECT `mysql_607` FROM `tbl_7`;
DELETE FROM bench_t_0 WHERE id = 0;
UPDATE bench_t_33 SET payload = 609 WHERE id = 1;
/* block header 610 */
BEGIN; SELECT 611; COMMIT;
SELECT nested FROM t WHERE id IN (612, 613, 614);
$dz$ dollar body 613 ; semicolon inside $dz$
SELECT * FROM "quoted_614" WHERE col = E'esc\'614';
SELECT 615 AS id, 'row_615' AS label;
$dz$ dollar body 616 ; semicolon inside $dz$
BEGIN; SELECT 617; COMMIT;
SELECT nested FROM t WHERE id IN (618, 619, 620);
# hash comment 619
DELETE FROM bench_t_12 WHERE id = 12;
SELECT [bracket_621] FROM [dbo].[tbl_21];
BEGIN; SELECT 622; COMMIT;
SELECT * FROM "quoted_623" WHERE col = E'esc\'623';
DELETE FROM bench_t_16 WHERE id = 0;
DELETE FROM bench_t_17 WHERE id = 1;
SELECT `mysql_626` FROM `tbl_26`;
SELECT * FROM "quoted_627" WHERE col = E'esc\'627';
SELECT nested FROM t WHERE id IN (628, 629, 630);
SELECT * FROM "quoted_629" WHERE col = E'esc\'629';
INSERT INTO bench_t_118 (id, payload) VALUES (630, 'v630');
UPDATE bench_t_55 SET payload = 631 WHERE id = 23;
SELECT 632 AS id, 'row_632' AS label;
WITH cte_633 AS (SELECT 633 AS n) SELECT n FROM cte_633;
$dz$ dollar body 634 ; semicolon inside $dz$
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT 636 AS id, 'row_636' AS label;
SELECT 637 AS id, 'row_637' AS label;
/* block header 638 */
SELECT * FROM "quoted_639" WHERE col = E'esc\'639';
SELECT `mysql_640` FROM `tbl_40`;
SELECT * FROM "quoted_641" WHERE col = E'esc\'641';
INSERT INTO bench_t_2 (id, payload) VALUES (642, 'v642');
BEGIN; SELECT 643; COMMIT;
UPDATE bench_t_4 SET payload = 644 WHERE id = 4;
DELETE FROM bench_t_5 WHERE id = 5;
# hash comment 646
SELECT [bracket_647] FROM [dbo].[tbl_7];
WITH cte_648 AS (SELECT 648 AS n) SELECT n FROM cte_648;
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
# hash comment 650
SELECT * FROM "quoted_651" WHERE col = E'esc\'651';
/* block header 652 */
SELECT * FROM "quoted_653" WHERE col = E'esc\'653';
# hash comment 654
$dz$ dollar body 655 ; semicolon inside $dz$
/* block header 656 */
$dz$ dollar body 657 ; semicolon inside $dz$
$dz$ dollar body 658 ; semicolon inside $dz$
INSERT INTO bench_t_19 (id, payload) VALUES (659, 'v659');
UPDATE bench_t_20 SET payload = 660 WHERE id = 20;
$dz$ dollar body 661 ; semicolon inside $dz$
INSERT INTO bench_t_22 (id, payload) VALUES (662, 'v662');
DELETE FROM bench_t_23 WHERE id = 7;
/* block header 664 */
SELECT * FROM "quoted_665" WHERE col = E'esc\'665';
/* block header 666 */
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
BEGIN; SELECT 668; COMMIT;
INSERT INTO bench_t_29 (id, payload) VALUES (669, 'v669');
# hash comment 670
UPDATE bench_t_31 SET payload = 671 WHERE id = 31;
SELECT `mysql_672` FROM `tbl_22`;
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT [bracket_674] FROM [dbo].[tbl_34];
-- line 675: deterministic comment
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
INSERT INTO bench_t_37 (id, payload) VALUES (677, 'v677');
BEGIN; SELECT 678; COMMIT;
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
UPDATE bench_t_40 SET payload = 680 WHERE id = 8;
BEGIN; SELECT 681; COMMIT;
# hash comment 682
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT `mysql_684` FROM `tbl_34`;
/* block header 685 */
SELECT * FROM "quoted_686" WHERE col = E'esc\'686';
WITH cte_687 AS (SELECT 687 AS n) SELECT n FROM cte_687;
SELECT nested FROM t WHERE id IN (688, 689, 690);
UPDATE bench_t_49 SET payload = 689 WHERE id = 17;
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
UPDATE bench_t_51 SET payload = 691 WHERE id = 19;
# hash comment 692
/* block header 693 */
SELECT nested FROM t WHERE id IN (694, 695, 696);
INSERT INTO bench_t_55 (id, payload) VALUES (695, 'v695');
SELECT `mysql_696` FROM `tbl_46`;
INSERT INTO bench_t_57 (id, payload) VALUES (697, 'v697');
SELECT 698 AS id, 'row_698' AS label;
SELECT `mysql_699` FROM `tbl_49`;
WITH cte_700 AS (SELECT 700 AS n) SELECT n FROM cte_700;
SELECT 701 AS id, 'row_701' AS label;
UPDATE bench_t_62 SET payload = 702 WHERE id = 30;
SELECT nested FROM t WHERE id IN (703, 704, 705);
SELECT `mysql_704` FROM `tbl_4`;
SELECT 705 AS id, 'row_705' AS label;
$dz$ dollar body 706 ; semicolon inside $dz$
SELECT nested FROM t WHERE id IN (707, 708, 709);
SELECT [bracket_708] FROM [dbo].[tbl_28];
BEGIN; SELECT 709; COMMIT;
UPDATE bench_t_6 SET payload = 710 WHERE id = 6;
UPDATE bench_t_7 SET payload = 711 WHERE id = 7;
SELECT 712 AS id, 'row_712' AS label;
BEGIN; SELECT 713; COMMIT;
$dz$ dollar body 714 ; semicolon inside $dz$
WITH cte_715 AS (SELECT 715 AS n) SELECT n FROM cte_715;
INSERT INTO bench_t_76 (id, payload) VALUES (716, 'v716');
UPDATE bench_t_13 SET payload = 717 WHERE id = 13;
SELECT [bracket_718] FROM [dbo].[tbl_38];
SELECT [bracket_719] FROM [dbo].[tbl_39];
SELECT nested FROM t WHERE id IN (720, 721, 722);
INSERT INTO bench_t_81 (id, payload) VALUES (721, 'v721');
$dz$ dollar body 722 ; semicolon inside $dz$
DELETE FROM bench_t_19 WHERE id = 3;
UPDATE bench_t_20 SET payload = 724 WHERE id = 20;
WITH cte_725 AS (SELECT 725 AS n) SELECT n FROM cte_725;
SELECT nested FROM t WHERE id IN (726, 727, 728);
$dz$ dollar body 727 ; semicolon inside $dz$
DELETE FROM bench_t_24 WHERE id = 8;
-- line 729: deterministic comment
SELECT * FROM "quoted_730" WHERE col = E'esc\'730';
SELECT nested FROM t WHERE id IN (731, 732, 733);
SELECT nested FROM t WHERE id IN (732, 733, 734);
-- line 733: deterministic comment
BEGIN; SELECT 734; COMMIT;
WITH cte_735 AS (SELECT 735 AS n) SELECT n FROM cte_735;
$dz$ dollar body 736 ; semicolon inside $dz$
SELECT nested FROM t WHERE id IN (737, 738, 739);
SELECT nested FROM t WHERE id IN (738, 739, 740);
SELECT [bracket_739] FROM [dbo].[tbl_19];
DELETE FROM bench_t_4 WHERE id = 4;
SELECT `mysql_741` FROM `tbl_41`;
-- line 742: deterministic comment
WITH cte_743 AS (SELECT 743 AS n) SELECT n FROM cte_743;
SELECT `mysql_744` FROM `tbl_44`;
$dz$ dollar body 745 ; semicolon inside $dz$
BEGIN; SELECT 746; COMMIT;
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
INSERT INTO bench_t_108 (id, payload) VALUES (748, 'O''Brien');
$dz$ dollar body 749 ; semicolon inside $dz$
/*
 * section 3
 * checksum 3df8
 */
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
INSERT INTO bench_t_116 (id, payload) VALUES (756, 'v756');
WITH cte_757 AS (SELECT 757 AS n) SELECT n FROM cte_757;
BEGIN; SELECT 758; COMMIT;
WITH cte_759 AS (SELECT 759 AS n) SELECT n FROM cte_759;
SELECT nested FROM t WHERE id IN (760, 761, 762);
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
$dz$ dollar body 762 ; semicolon inside $dz$
WITH cte_763 AS (SELECT 763 AS n) SELECT n FROM cte_763;
BEGIN; SELECT 764; COMMIT;
DELETE FROM bench_t_29 WHERE id = 13;
DELETE FROM bench_t_30 WHERE id = 14;
-- line 767: deterministic comment
-- line 768: deterministic comment
# hash comment 769
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
DELETE FROM bench_t_3 WHERE id = 3;
WITH cte_772 AS (SELECT 772 AS n) SELECT n FROM cte_772;
SELECT * FROM "quoted_773" WHERE col = E'esc\'773';
# hash comment 774
/* block header 775 */
# hash comment 776
$dz$ dollar body 777 ; semicolon inside $dz$
BEGIN; SELECT 778; COMMIT;
DELETE FROM bench_t_11 WHERE id = 11;
/* block header 780 */
SELECT 781 AS id, 'row_781' AS label;
WITH cte_782 AS (SELECT 782 AS n) SELECT n FROM cte_782;
-- line 783: deterministic comment
# hash comment 784
SELECT nested FROM t WHERE id IN (785, 786, 787);
UPDATE bench_t_18 SET payload = 786 WHERE id = 18;
/* block header 787 */
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
DELETE FROM bench_t_21 WHERE id = 5;
$dz$ dollar body 790 ; semicolon inside $dz$
SELECT * FROM "quoted_791" WHERE col = E'esc\'791';
SELECT * FROM "quoted_792" WHERE col = E'esc\'792';
SELECT 793 AS id, 'row_793' AS label;
SELECT `mysql_794` FROM `tbl_44`;
BEGIN; SELECT 795; COMMIT;
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT * FROM "quoted_797" WHERE col = E'esc\'797';
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT * FROM "quoted_799" WHERE col = E'esc\'799';
WITH cte_800 AS (SELECT 800 AS n) SELECT n FROM cte_800;
UPDATE bench_t_33 SET payload = 801 WHERE id = 1;
-- line 802: deterministic comment
/* block header 803 */
WITH cte_804 AS (SELECT 804 AS n) SELECT n FROM cte_804;
SELECT 805 AS id, 'row_805' AS label;
DELETE FROM bench_t_6 WHERE id = 6;
DELETE FROM bench_t_7 WHERE id = 7;
BEGIN; SELECT 808; COMMIT;
SELECT * FROM "quoted_809" WHERE col = E'esc\'809';
SELECT 810 AS id, 'row_810' AS label;
UPDATE bench_t_43 SET payload = 811 WHERE id = 11;
-- line 812: deterministic comment
UPDATE bench_t_45 SET payload = 813 WHERE id = 13;
BEGIN; SELECT 814; COMMIT;
DELETE FROM bench_t_15 WHERE id = 15;
SELECT nested FROM t WHERE id IN (816, 817, 818);
DELETE FROM bench_t_17 WHERE id = 1;
SELECT * FROM "quoted_818" WHERE col = E'esc\'818';
BEGIN; SELECT 819; COMMIT;
INSERT INTO bench_t_52 (id, payload) VALUES (820, 'v820');
SELECT `mysql_821` FROM `tbl_21`;
# hash comment 822
SELECT `mysql_823` FROM `tbl_23`;
INSERT INTO bench_t_56 (id, payload) VALUES (824, 'v824');
$dz$ dollar body 825 ; semicolon inside $dz$
$dz$ dollar body 826 ; semicolon inside $dz$
DELETE FROM bench_t_27 WHERE id = 11;
INSERT INTO bench_t_60 (id, payload) VALUES (828, 'v828');
DELETE FROM bench_t_29 WHERE id = 13;
UPDATE bench_t_62 SET payload = 830 WHERE id = 30;
SELECT * FROM "quoted_831" WHERE col = E'esc\'831';
INSERT INTO bench_t_64 (id, payload) VALUES (832, 'v832');
UPDATE bench_t_1 SET payload = 833 WHERE id = 1;
-- line 834: deterministic comment
$dz$ dollar body 835 ; semicolon inside $dz$
# hash comment 836
$dz$ dollar body 837 ; semicolon inside $dz$
BEGIN; SELECT 838; COMMIT;
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
BEGIN; SELECT 840; COMMIT;
# hash comment 841
SELECT `mysql_842` FROM `tbl_42`;
$dz$ dollar body 843 ; semicolon inside $dz$
-- line 844: deterministic comment
UPDATE bench_t_13 SET payload = 845 WHERE id = 13;
/* block header 846 */
SELECT [bracket_847] FROM [dbo].[tbl_7];
$dz$ dollar body 848 ; semicolon inside $dz$
INSERT INTO bench_t_81 (id, payload) VALUES (849, 'v849');
$dz$ dollar body 850 ; semicolon inside $dz$
SELECT 851 AS id, 'row_851' AS label;
SELECT nested FROM t WHERE id IN (852, 853, 854);
BEGIN; SELECT 853; COMMIT;
SELECT 854 AS id, 'row_854' AS label;
-- line 855: deterministic comment
SELECT 856 AS id, 'row_856' AS label;
BEGIN; SELECT 857; COMMIT;
WITH cte_858 AS (SELECT 858 AS n) SELECT n FROM cte_858;
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
$dz$ dollar body 860 ; semicolon inside $dz$
DELETE FROM bench_t_29 WHERE id = 13;
# hash comment 862
/* block header 863 */
UPDATE bench_t_32 SET payload = 864 WHERE id = 0;
UPDATE bench_t_33 SET payload = 865 WHERE id = 1;
BEGIN; SELECT 866; COMMIT;
$dz$ dollar body 867 ; semicolon inside $dz$
/* block header 868 */
DELETE FROM bench_t_5 WHERE id = 5;
DELETE FROM bench_t_6 WHERE id = 6;
SELECT [bracket_871] FROM [dbo].[tbl_31];
SELECT 872 AS id, 'row_872' AS label;
INSERT INTO bench_t_105 (id, payload) VALUES (873, 'v873');
SELECT [bracket_874] FROM [dbo].[tbl_34];
/* block header 875 */
DELETE FROM bench_t_12 WHERE id = 12;
BEGIN; SELECT 877; COMMIT;
$dz$ dollar body 878 ; semicolon inside $dz$
SELECT 879 AS id, 'row_879' AS label;
INSERT INTO bench_t_112 (id, payload) VALUES (880, 'O''Brien');
# hash comment 881
SELECT * FROM "quoted_882" WHERE col = E'esc\'882';
$dz$ dollar body 883 ; semicolon inside $dz$
# hash comment 884
UPDATE bench_t_53 SET payload = 885 WHERE id = 21;
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
WITH cte_887 AS (SELECT 887 AS n) SELECT n FROM cte_887;
SELECT 888 AS id, 'row_888' AS label;
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT [bracket_890] FROM [dbo].[tbl_10];
SELECT [bracket_891] FROM [dbo].[tbl_11];
$dz$ dollar body 892 ; semicolon inside $dz$
SELECT [bracket_893] FROM [dbo].[tbl_13];
SELECT [bracket_894] FROM [dbo].[tbl_14];
WITH cte_895 AS (SELECT 895 AS n) SELECT n FROM cte_895;
INSERT INTO bench_t_0 (id, payload) VALUES (896, 'v896');
UPDATE bench_t_1 SET payload = 897 WHERE id = 1;
SELECT nested FROM t WHERE id IN (898, 899, 900);
-- line 899: deterministic comment
INSERT INTO bench_t_4 (id, payload) VALUES (900, 'v900');
SELECT [bracket_901] FROM [dbo].[tbl_21];
-- line 902: deterministic comment
DELETE FROM bench_t_7 WHERE id = 7;
SELECT [bracket_904] FROM [dbo].[tbl_24];
-- line 905: deterministic comment
-- line 906: deterministic comment
SELECT * FROM "quoted_907" WHERE col = E'esc\'907';
SELECT `mysql_908` FROM `tbl_8`;
SELECT [bracket_909] FROM [dbo].[tbl_29];
SELECT 910 AS id, 'row_910' AS label;
SELECT 911 AS id, 'row_911' AS label;
SELECT `mysql_912` FROM `tbl_12`;
UPDATE bench_t_17 SET payload = 913 WHERE id = 17;
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT nested FROM t WHERE id IN (915, 916, 917);
SELECT nested FROM t WHERE id IN (916, 917, 918);
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
INSERT INTO bench_t_22 (id, payload) VALUES (918, 'v918');
SELECT [bracket_919] FROM [dbo].[tbl_39];
INSERT INTO bench_t_24 (id, payload) VALUES (920, 'v920');
WITH cte_921 AS (SELECT 921 AS n) SELECT n FROM cte_921;
INSERT INTO bench_t_26 (id, payload) VALUES (922, 'v922');
# hash comment 923
BEGIN; SELECT 924; COMMIT;
$dz$ dollar body 925 ; semicolon inside $dz$
-- line 926: deterministic comment
BEGIN; SELECT 927; COMMIT;
SELECT nested FROM t WHERE id IN (928, 929, 930);
# hash comment 929
SELECT * FROM "quoted_930" WHERE col = E'esc\'930';
SELECT nested FROM t WHERE id IN (931, 932, 933);
WITH cte_932 AS (SELECT 932 AS n) SELECT n FROM cte_932;
/* block header 933 */
$dz$ dollar body 934 ; semicolon inside $dz$
SELECT 935 AS id, 'row_935' AS label;
WITH cte_936 AS (SELECT 936 AS n) SELECT n FROM cte_936;
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
# hash comment 938
$dz$ dollar body 939 ; semicolon inside $dz$
INSERT INTO bench_t_44 (id, payload) VALUES (940, 'v940');
SELECT `mysql_941` FROM `tbl_41`;
SELECT [bracket_942] FROM [dbo].[tbl_22];
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
-- line 944: deterministic comment
SELECT 945 AS id, 'row_945' AS label;
DELETE FROM bench_t_18 WHERE id = 2;
/* block header 947 */
WITH cte_948 AS (SELECT 948 AS n) SELECT n FROM cte_948;
SELECT [bracket_949] FROM [dbo].[tbl_29];
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
-- line 951: deterministic comment
$dz$ dollar body 952 ; semicolon inside $dz$
SELECT `mysql_953` FROM `tbl_3`;
BEGIN; SELECT 954; COMMIT;
DELETE FROM bench_t_27 WHERE id = 11;
BEGIN; SELECT 956; COMMIT;
DELETE FROM bench_t_29 WHERE id = 13;
SELECT `mysql_958` FROM `tbl_8`;
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
# hash comment 960
# hash comment 961
SELECT nested FROM t WHERE id IN (962, 963, 964);
SELECT `mysql_963` FROM `tbl_13`;
SELECT nested FROM t WHERE id IN (964, 965, 966);
# hash comment 965
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
/* block header 967 */
-- line 968: deterministic comment
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
-- line 970: deterministic comment
SELECT `mysql_971` FROM `tbl_21`;
WITH cte_972 AS (SELECT 972 AS n) SELECT n FROM cte_972;
SELECT [bracket_973] FROM [dbo].[tbl_13];
$dz$ dollar body 974 ; semicolon inside $dz$
SELECT 975 AS id, 'row_975' AS label;
WITH cte_976 AS (SELECT 976 AS n) SELECT n FROM cte_976;
/* block header 977 */
# hash comment 978
SELECT 979 AS id, 'row_979' AS label;
SELECT nested FROM t WHERE id IN (980, 981, 982);
SELECT `mysql_981` FROM `tbl_31`;
SELECT [bracket_982] FROM [dbo].[tbl_22];
# hash comment 983
SELECT [bracket_984] FROM [dbo].[tbl_24];
SELECT nested FROM t WHERE id IN (985, 986, 987);
UPDATE bench_t_26 SET payload = 986 WHERE id = 26;
SELECT * FROM "quoted_987" WHERE col = E'esc\'987';
/* block header 988 */
SELECT nested FROM t WHERE id IN (989, 990, 991);
DELETE FROM bench_t_30 WHERE id = 14;
SELECT nested FROM t WHERE id IN (991, 992, 993);
BEGIN; SELECT 992; COMMIT;
SELECT nested FROM t WHERE id IN (993, 994, 995);
SELECT * FROM "quoted_994" WHERE col = E'esc\'994';
SELECT 995 AS id, 'row_995' AS label;
-- line 996: deterministic comment
SELECT 997 AS id, 'row_997' AS label;
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
UPDATE bench_t_39 SET payload = 999 WHERE id = 7;
/*
 * section 4
 * checksum 423f
 */
BEGIN; SELECT 1000; COMMIT;
BEGIN; SELECT 1005; COMMIT;
SELECT [bracket_1006] FROM [dbo].[tbl_6];
DELETE FROM bench_t_15 WHERE id = 15;
$dz$ dollar body 1008 ; semicolon inside $dz$
SELECT * FROM "quoted_1009" WHERE col = E'esc\'1009';
-- line 1010: deterministic comment
SELECT 1011 AS id, 'row_1011' AS label;
/* block header 1012 */
INSERT INTO bench_t_117 (id, payload) VALUES (1013, 'v1013');
INSERT INTO bench_t_118 (id, payload) VALUES (1014, 'v1014');
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
/* block header 1016 */
-- line 1017: deterministic comment
UPDATE bench_t_58 SET payload = 1018 WHERE id = 26;
UPDATE bench_t_59 SET payload = 1019 WHERE id = 27;
SELECT `mysql_1020` FROM `tbl_20`;
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
/* block header 1022 */
DELETE FROM bench_t_31 WHERE id = 15;
SELECT [bracket_1024] FROM [dbo].[tbl_24];
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
# hash comment 1026
-- line 1027: deterministic comment
-- line 1028: deterministic comment
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
INSERT INTO bench_t_6 (id, payload) VALUES (1030, 'v1030');
SELECT 1031 AS id, 'row_1031' AS label;
INSERT INTO bench_t_8 (id, payload) VALUES (1032, 'v1032');
/* block header 1033 */
WITH cte_1034 AS (SELECT 1034 AS n) SELECT n FROM cte_1034;
/* block header 1035 */
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT 1037 AS id, 'row_1037' AS label;
SELECT nested FROM t WHERE id IN (1038, 1039, 1040);
SELECT 1039 AS id, 'row_1039' AS label;
SELECT 1040 AS id, 'row_1040' AS label;
DELETE FROM bench_t_17 WHERE id = 1;
DELETE FROM bench_t_18 WHERE id = 2;
SELECT * FROM "quoted_1043" WHERE col = E'esc\'1043';
SELECT 1044 AS id, 'row_1044' AS label;
SELECT `mysql_1045` FROM `tbl_45`;
SELECT * FROM "quoted_1046" WHERE col = E'esc\'1046';
SELECT [bracket_1047] FROM [dbo].[tbl_7];
BEGIN; SELECT 1048; COMMIT;
/* block header 1049 */
BEGIN; SELECT 1050; COMMIT;
/* block header 1051 */
/* block header 1052 */
DELETE FROM bench_t_29 WHERE id = 13;
INSERT INTO bench_t_30 (id, payload) VALUES (1054, 'v1054');
SELECT nested FROM t WHERE id IN (1055, 1056, 1057);
SELECT 1056 AS id, 'row_1056' AS label;
-- line 1057: deterministic comment
/* block header 1058 */
/* block header 1059 */
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT * FROM "quoted_1061" WHERE col = E'esc\'1061';
SELECT `mysql_1062` FROM `tbl_12`;
BEGIN; SELECT 1063; COMMIT;
INSERT INTO bench_t_40 (id, payload) VALUES (1064, 'v1064');
SELECT * FROM "quoted_1065" WHERE col = E'esc\'1065';
# hash comment 1066
/* block header 1067 */
$dz$ dollar body 1068 ; semicolon inside $dz$
/* block header 1069 */
/* block header 1070 */
SELECT nested FROM t WHERE id IN (1071, 1072, 1073);
UPDATE bench_t_48 SET payload = 1072 WHERE id = 16;
DELETE FROM bench_t_17 WHERE id = 1;
# hash comment 1074
SELECT * FROM "quoted_1075" WHERE col = E'esc\'1075';
SELECT * FROM "quoted_1076" WHERE col = E'esc\'1076';
/* block header 1077 */
WITH cte_1078 AS (SELECT 1078 AS n) SELECT n FROM cte_1078;
INSERT INTO bench_t_55 (id, payload) VALUES (1079, 'v1079');
-- line 1080: deterministic comment
SELECT 1081 AS id, 'row_1081' AS label;
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
$dz$ dollar body 1083 ; semicolon inside $dz$
WITH cte_1084 AS (SELECT 1084 AS n) SELECT n FROM cte_1084;
-- line 1085: deterministic comment
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
INSERT INTO bench_t_63 (id, payload) VALUES (1087, 'v1087');
UPDATE bench_t_0 SET payload = 1088 WHERE id = 0;
INSERT INTO bench_t_65 (id, payload) VALUES (1089, 'O''Brien');
UPDATE bench_t_2 SET payload = 1090 WHERE id = 2;
INSERT INTO bench_t_67 (id, payload) VALUES (1091, 'v1091');
UPDATE bench_t_4 SET payload = 1092 WHERE id = 4;
SELECT [bracket_1093] FROM [dbo].[tbl_13];
WITH cte_1094 AS (SELECT 1094 AS n) SELECT n FROM cte_1094;
UPDATE bench_t_7 SET payload = 1095 WHERE id = 7;
# hash comment 1096
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
$dz$ dollar body 1098 ; semicolon inside $dz$
# hash comment 1099
# hash comment 1100
SELECT 1101 AS id, 'row_1101' AS label;
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
UPDATE bench_t_15 SET payload = 1103 WHERE id = 15;
INSERT INTO bench_t_80 (id, payload) VALUES (1104, 'v1104');
BEGIN; SELECT 1105; COMMIT;
SELECT nested FROM t WHERE id IN (1106, 1107, 1108);
SELECT * FROM "quoted_1107" WHERE col = E'esc\'1107';
SELECT * FROM "quoted_1108" WHERE col = E'esc\'1108';
SELECT `mysql_1109` FROM `tbl_9`;
-- line 1110: deterministic comment
WITH cte_1111 AS (SELECT 1111 AS n) SELECT n FROM cte_1111;
UPDATE bench_t_24 SET payload = 1112 WHERE id = 24;
BEGIN; SELECT 1113; COMMIT;
SELECT 1114 AS id, 'row_1114' AS label;
INSERT INTO bench_t_91 (id, payload) VALUES (1115, 'v1115');
WITH cte_1116 AS (SELECT 1116 AS n) SELECT n FROM cte_1116;
SELECT 1117 AS id, 'row_1117' AS label;
SELECT nested FROM t WHERE id IN (1118, 1119, 1120);
INSERT INTO bench_t_95 (id, payload) VALUES (1119, 'v1119');
UPDATE bench_t_32 SET payload = 1120 WHERE id = 0;
WITH cte_1121 AS (SELECT 1121 AS n) SELECT n FROM cte_1121;
SELECT `mysql_1122` FROM `tbl_22`;
SELECT `mysql_1123` FROM `tbl_23`;
SELECT 1124 AS id, 'row_1124' AS label;
-- line 1125: deterministic comment
WITH cte_1126 AS (SELECT 1126 AS n) SELECT n FROM cte_1126;
SELECT [bracket_1127] FROM [dbo].[tbl_7];
$dz$ dollar body 1128 ; semicolon inside $dz$
SELECT `mysql_1129` FROM `tbl_29`;
SELECT * FROM "quoted_1130" WHERE col = E'esc\'1130';
SELECT nested FROM t WHERE id IN (1131, 1132, 1133);
SELECT nested FROM t WHERE id IN (1132, 1133, 1134);
-- line 1133: deterministic comment
WITH cte_1134 AS (SELECT 1134 AS n) SELECT n FROM cte_1134;
-- line 1135: deterministic comment
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
# hash comment 1137
/* block header 1138 */
UPDATE bench_t_51 SET payload = 1139 WHERE id = 19;
SELECT [bracket_1140] FROM [dbo].[tbl_20];
SELECT [bracket_1141] FROM [dbo].[tbl_21];
WITH cte_1142 AS (SELECT 1142 AS n) SELECT n FROM cte_1142;
SELECT [bracket_1143] FROM [dbo].[tbl_23];
SELECT * FROM "quoted_1144" WHERE col = E'esc\'1144';
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT [bracket_1147] FROM [dbo].[tbl_27];
SELECT 1148 AS id, 'row_1148' AS label;
-- line 1149: deterministic comment
-- line 1150: deterministic comment
-- line 1151: deterministic comment
WITH cte_1152 AS (SELECT 1152 AS n) SELECT n FROM cte_1152;
INSERT INTO bench_t_1 (id, payload) VALUES (1153, 'v1153');
SELECT nested FROM t WHERE id IN (1154, 1155, 1156);
SELECT nested FROM t WHERE id IN (1155, 1156, 1157);
UPDATE bench_t_4 SET payload = 1156 WHERE id = 4;
SELECT * FROM "quoted_1157" WHERE col = E'esc\'1157';
SELECT [bracket_1158] FROM [dbo].[tbl_38];
INSERT INTO bench_t_7 (id, payload) VALUES (1159, 'v1159');
BEGIN; SELECT 1160; COMMIT;
BEGIN; SELECT 1161; COMMIT;
BEGIN; SELECT 1162; COMMIT;
UPDATE bench_t_11 SET payload = 1163 WHERE id = 11;
UPDATE bench_t_12 SET payload = 1164 WHERE id = 12;
INSERT INTO bench_t_13 (id, payload) VALUES (1165, 'v1165');
$dz$ dollar body 1166 ; semicolon inside $dz$
INSERT INTO bench_t_15 (id, payload) VALUES (1167, 'v1167');
UPDATE bench_t_16 SET payload = 1168 WHERE id = 16;
DELETE FROM bench_t_17 WHERE id = 1;
BEGIN; SELECT 1170; COMMIT;
SELECT `mysql_1171` FROM `tbl_21`;
BEGIN; SELECT 1172; COMMIT;
-- line 1173: deterministic comment
-- line 1174: deterministic comment
DELETE FROM bench_t_23 WHERE id = 7;
SELECT nested FROM t WHERE id IN (1176, 1177, 1178);
SELECT `mysql_1177` FROM `tbl_27`;
SELECT nested FROM t WHERE id IN (1178, 1179, 1180);
DELETE FROM bench_t_27 WHERE id = 11;
$dz$ dollar body 1180 ; semicolon inside $dz$
SELECT `mysql_1181` FROM `tbl_31`;
SELECT [bracket_1182] FROM [dbo].[tbl_22];
UPDATE bench_t_31 SET payload = 1183 WHERE id = 31;
-- line 1184: deterministic comment
SELECT 1185 AS id, 'row_1185' AS label;
$dz$ dollar body 1186 ; semicolon inside $dz$
-- line 1187: deterministic comment
SELECT [bracket_1188] FROM [dbo].[tbl_28];
-- line 1189: deterministic comment
SELECT `mysql_1190` FROM `tbl_40`;
DELETE FROM bench_t_7 WHERE id = 7;
# hash comment 1192
INSERT INTO bench_t_41 (id, payload) VALUES (1193, 'v1193');
UPDATE bench_t_42 SET payload = 1194 WHERE id = 10;
BEGIN; SELECT 1195; COMMIT;
WITH cte_1196 AS (SELECT 1196 AS n) SELECT n FROM cte_1196;
UPDATE bench_t_45 SET payload = 1197 WHERE id = 13;
# hash comment 1198
UPDATE bench_t_47 SET payload = 1199 WHERE id = 15;
SELECT * FROM "quoted_1200" WHERE col = E'esc\'1200';
DELETE FROM bench_t_17 WHERE id = 1;
SELECT [bracket_1202] FROM [dbo].[tbl_2];
SELECT 1203 AS id, 'row_1203' AS label;
DELETE FROM bench_t_20 WHERE id = 4;
/* block header 1205 */
$dz$ dollar body 1206 ; semicolon inside $dz$
SELECT * FROM "quoted_1207" WHERE col = E'esc\'1207';
SELECT nested FROM t WHERE id IN (1208, 1209, 1210);
-- line 1209: deterministic comment
-- line 1210: deterministic comment
UPDATE bench_t_59 SET payload = 1211 WHERE id = 27;
INSERT INTO bench_t_60 (id, payload) VALUES (1212, 'v1212');
SELECT * FROM "quoted_1213" WHERE col = E'esc\'1213';
/* block header 1214 */
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
DELETE FROM bench_t_0 WHERE id = 0;
-- line 1217: deterministic comment
SELECT [bracket_1218] FROM [dbo].[tbl_18];
SELECT nested FROM t WHERE id IN (1219, 1220, 1221);
-- line 1220: deterministic comment
SELECT nested FROM t WHERE id IN (1221, 1222, 1223);
SELECT nested FROM t WHERE id IN (1222, 1223, 1224);
BEGIN; SELECT 1223; COMMIT;
SELECT 1224 AS id, 'row_1224' AS label;
$dz$ dollar body 1225 ; semicolon inside $dz$
SELECT nested FROM t WHERE id IN (1226, 1227, 1228);
BEGIN; SELECT 1227; COMMIT;
SELECT [bracket_1228] FROM [dbo].[tbl_28];
SELECT nested FROM t WHERE id IN (1229, 1230, 1231);
INSERT INTO bench_t_78 (id, payload) VALUES (1230, 'v1230');
-- line 1231: deterministic comment
BEGIN; SELECT 1232; COMMIT;
SELECT nested FROM t WHERE id IN (1233, 1234, 1235);
SELECT 1234 AS id, 'row_1234' AS label;
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
INSERT INTO bench_t_85 (id, payload) VALUES (1237, 'v1237');
SELECT 1238 AS id, 'row_1238' AS label;
SELECT nested FROM t WHERE id IN (1239, 1240, 1241);
DELETE FROM bench_t_24 WHERE id = 8;
DELETE FROM bench_t_25 WHERE id = 9;
SELECT 1242 AS id, 'row_1242' AS label;
UPDATE bench_t_27 SET payload = 1243 WHERE id = 27;
SELECT [bracket_1244] FROM [dbo].[tbl_4];
# hash comment 1245
SELECT `mysql_1246` FROM `tbl_46`;
$dz$ dollar body 1247 ; semicolon inside $dz$
SELECT nested FROM t WHERE id IN (1248, 1249, 1250);
UPDATE bench_t_33 SET payload = 1249 WHERE id = 1;
/*
 * section 5
 * checksum c9de
 */
$dz$ dollar body 1250 ; semicolon inside $dz$
UPDATE bench_t_39 SET payload = 1255 WHERE id = 7;
$dz$ dollar body 1256 ; semicolon inside $dz$
SELECT 1257 AS id, 'row_1257' AS label;
SELECT nested FROM t WHERE id IN (1258, 1259, 1260);
SELECT * FROM "quoted_1259" WHERE col = E'esc\'1259';
SELECT 1260 AS id, 'row_1260' AS label;
WITH cte_1261 AS (SELECT 1261 AS n) SELECT n FROM cte_1261;
SELECT * FROM "quoted_1262" WHERE col = E'esc\'1262';
$dz$ dollar body 1263 ; semicolon inside $dz$
-- line 1264: deterministic comment
SELECT * FROM "quoted_1265" WHERE col = E'esc\'1265';
SELECT [bracket_1266] FROM [dbo].[tbl_26];
SELECT 1267 AS id, 'row_1267' AS label;
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT [bracket_1269] FROM [dbo].[tbl_29];
WITH cte_1270 AS (SELECT 1270 AS n) SELECT n FROM cte_1270;
SELECT nested FROM t WHERE id IN (1271, 1272, 1273);
SELECT nested FROM t WHERE id IN (1272, 1273, 1274);
INSERT INTO bench_t_121 (id, payload) VALUES (1273, 'v1273');
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT 1275 AS id, 'row_1275' AS label;
/* block header 1276 */
SELECT * FROM "quoted_1277" WHERE col = E'esc\'1277';
/* block header 1278 */
$dz$ dollar body 1279 ; semicolon inside $dz$
-- line 1280: deterministic comment
SELECT [bracket_1281] FROM [dbo].[tbl_1];
SELECT * FROM "quoted_1282" WHERE col = E'esc\'1282';
SELECT 1283 AS id, 'row_1283' AS label;
SELECT * FROM "quoted_1284" WHERE col = E'esc\'1284';
SELECT [bracket_1285] FROM [dbo].[tbl_5];
INSERT INTO bench_t_6 (id, payload) VALUES (1286, 'v1286');
SELECT [bracket_1287] FROM [dbo].[tbl_7];
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
# hash comment 1289
WITH cte_1290 AS (SELECT 1290 AS n) SELECT n FROM cte_1290;
/* block header 1291 */
# hash comment 1292
BEGIN; SELECT 1293; COMMIT;
$dz$ dollar body 1294 ; semicolon inside $dz$
DELETE FROM bench_t_15 WHERE id = 15;
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
$dz$ dollar body 1297 ; semicolon inside $dz$
/* block header 1298 */
SELECT 1299 AS id, 'row_1299' AS label;
-- line 1300: deterministic comment
BEGIN; SELECT 1301; COMMIT;
$dz$ dollar body 1302 ; semicolon inside $dz$
BEGIN; SELECT 1303; COMMIT;
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT nested FROM t WHERE id IN (1305, 1306, 1307);
# hash comment 1306
SELECT [bracket_1307] FROM [dbo].[tbl_27];
DELETE FROM bench_t_28 WHERE id = 12;
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
-- line 1310: deterministic comment
INSERT INTO bench_t_31 (id, payload) VALUES (1311, 'v1311');
SELECT `mysql_1312` FROM `tbl_12`;
INSERT INTO bench_t_33 (id, payload) VALUES (1313, 'v1313');
BEGIN; SELECT 1314; COMMIT;
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT `mysql_1316` FROM `tbl_16`;
SELECT nested FROM t WHERE id IN (1317, 1318, 1319);
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT [bracket_1319] FROM [dbo].[tbl_39];
INSERT INTO bench_t_40 (id, payload) VALUES (1320, 'O''Brien');
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
-- line 1322: deterministic comment
SELECT * FROM "quoted_1323" WHERE col = E'esc\'1323';
# hash comment 1324
DELETE FROM bench_t_13 WHERE id = 13;
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT `mysql_1327` FROM `tbl_27`;
SELECT `mysql_1328` FROM `tbl_28`;
$dz$ dollar body 1329 ; semicolon inside $dz$
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
$dz$ dollar body 1331 ; semicolon inside $dz$
UPDATE bench_t_52 SET payload = 1332 WHERE id = 20;
SELECT 1333 AS id, 'row_1333' AS label;
BEGIN; SELECT 1334; COMMIT;
$dz$ dollar body 1335 ; semicolon inside $dz$
BEGIN; SELECT 1336; COMMIT;
UPDATE bench_t_57 SET payload = 1337 WHERE id = 25;
WITH cte_1338 AS (SELECT 1338 AS n) SELECT n FROM cte_1338;
SELECT 1339 AS id, 'row_1339' AS label;
WITH cte_1340 AS (SELECT 1340 AS n) SELECT n FROM cte_1340;
/* block header 1341 */
# hash comment 1342
UPDATE bench_t_63 SET payload = 1343 WHERE id = 31;
BEGIN; SELECT 1344; COMMIT;
BEGIN; SELECT 1345; COMMIT;
SELECT * FROM "quoted_1346" WHERE col = E'esc\'1346';
SELECT nested FROM t WHERE id IN (1347, 1348, 1349);
/* block header 1348 */
INSERT INTO bench_t_69 (id, payload) VALUES (1349, 'v1349');
INSERT INTO bench_t_70 (id, payload) VALUES (1350, 'v1350');
SELECT * FROM "quoted_1351" WHERE col = E'esc\'1351';
BEGIN; SELECT 1352; COMMIT;
# hash comment 1353
-- line 1354: deterministic comment
/* block header 1355 */
BEGIN; SELECT 1356; COMMIT;
SELECT nested FROM t WHERE id IN (1357, 1358, 1359);
/* block header 1358 */
$dz$ dollar body 1359 ; semicolon inside $dz$
WITH cte_1360 AS (SELECT 1360 AS n) SELECT n FROM cte_1360;
BEGIN; SELECT 1361; COMMIT;
UPDATE bench_t_18 SET payload = 1362 WHERE id = 18;
$dz$ dollar body 1363 ; semicolon inside $dz$
SELECT `mysql_1364` FROM `tbl_14`;
WITH cte_1365 AS (SELECT 1365 AS n) SELECT n FROM cte_1365;
$dz$ dollar body 1366 ; semicolon inside $dz$
SELECT 1367 AS id, 'row_1367' AS label;
DELETE FROM bench_t_24 WHERE id = 8;
SELECT [bracket_1369] FROM [dbo].[tbl_9];
BEGIN; SELECT 1370; COMMIT;
WITH cte_1371 AS (SELECT 1371 AS n) SELECT n FROM cte_1371;
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
/* block header 1373 */
# hash comment 1374
WITH cte_1375 AS (SELECT 1375 AS n) SELECT n FROM cte_1375;
INSERT INTO bench_t_96 (id, payload) VALUES (1376, 'v1376');
WITH cte_1377 AS (SELECT 1377 AS n) SELECT n FROM cte_1377;
UPDATE bench_t_34 SET payload = 1378 WHERE id = 2;
# hash comment 1379
DELETE FROM bench_t_4 WHERE id = 4;
SELECT nested FROM t WHERE id IN (1381, 1382, 1383);
SELECT [bracket_1382] FROM [dbo].[tbl_22];
SELECT nested FROM t WHERE id IN (1383, 1384, 1385);
DELETE FROM bench_t_8 WHERE id = 8;
SELECT 1385 AS id, 'row_1385' AS label;
SELECT nested FROM t WHERE id IN (1386, 1387, 1388);
-- line 1387: deterministic comment
INSERT INTO bench_t_108 (id, payload) VALUES (1388, 'v1388');
-- line 1389: deterministic comment
DELETE FROM bench_t_14 WHERE id = 14;
SELECT `mysql_1391` FROM `tbl_41`;
INSERT INTO bench_t_112 (id, payload) VALUES (1392, 'v1392');
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
INSERT INTO bench_t_114 (id, payload) VALUES (1394, 'v1394');
DELETE FROM bench_t_19 WHERE id = 3;
/* block header 1396 */
UPDATE bench_t_53 SET payload = 1397 WHERE id = 21;
/* block header 1398 */
/* block header 1399 */
WITH cte_1400 AS (SELECT 1400 AS n) SELECT n FROM cte_1400;
-- line 1401: deterministic comment
# hash comment 1402
SELECT 1403 AS id, 'row_1403' AS label;
UPDATE bench_t_60 SET payload = 1404 WHERE id = 28;
$dz$ dollar body 1405 ; semicolon inside $dz$
INSERT INTO bench_t_126 (id, payload) VALUES (1406, 'v1406');
UPDATE bench_t_63 SET payload = 1407 WHERE id = 31;
SELECT `mysql_1408` FROM `tbl_8`;
INSERT INTO bench_t_1 (id, payload) VALUES (1409, 'v1409');
DELETE FROM bench_t_2 WHERE id = 2;
SELECT 1411 AS id, 'row_1411' AS label;
SELECT [bracket_1412] FROM [dbo].[tbl_12];
UPDATE bench_t_5 SET payload = 1413 WHERE id = 5;
SELECT * FROM "quoted_1414" WHERE col = E'esc\'1414';
INSERT INTO bench_t_7 (id, payload) VALUES (1415, 'v1415');
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
$dz$ dollar body 1417 ; semicolon inside $dz$
$dz$ dollar body 1418 ; semicolon inside $dz$
SELECT * FROM "quoted_1419" WHERE col = E'esc\'1419';
SELECT `mysql_1420` FROM `tbl_20`;
$dz$ dollar body 1421 ; semicolon inside $dz$
# hash comment 1422
$dz$ dollar body 1423 ; semicolon inside $dz$
SELECT nested FROM t WHERE id IN (1424, 1425, 1426);
# hash comment 1425
UPDATE bench_t_18 SET payload = 1426 WHERE id = 18;
SELECT 1427 AS id, 'row_1427' AS label;
/* block header 1428 */
SELECT * FROM "quoted_1429" WHERE col = E'esc\'1429';
WITH cte_1430 AS (SELECT 1430 AS n) SELECT n FROM cte_1430;
SELECT nested FROM t WHERE id IN (1431, 1432, 1433);
INSERT INTO bench_t_24 (id, payload) VALUES (1432, 'v1432');
INSERT INTO bench_t_25 (id, payload) VALUES (1433, 'v1433');
SELECT * FROM "quoted_1434" WHERE col = E'esc\'1434';
-- line 1435: deterministic comment
SELECT nested FROM t WHERE id IN (1436, 1437, 1438);
# hash comment 1437
UPDATE bench_t_30 SET payload = 1438 WHERE id = 30;
SELECT 1439 AS id, 'row_1439' AS label;
-- line 1440: deterministic comment
# hash comment 1441
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT `mysql_1443` FROM `tbl_43`;
WITH cte_1444 AS (SELECT 1444 AS n) SELECT n FROM cte_1444;
SELECT `mysql_1445` FROM `tbl_45`;
# hash comment 1446
/* block header 1447 */
BEGIN; SELECT 1448; COMMIT;
SELECT nested FROM t WHERE id IN (1449, 1450, 1451);
BEGIN; SELECT 1450; COMMIT;
$dz$ dollar body 1451 ; semicolon inside $dz$
$dz$ dollar body 1452 ; semicolon inside $dz$
# hash comment 1453
INSERT INTO bench_t_46 (id, payload) VALUES (1454, 'v1454');
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
BEGIN; SELECT 1456; COMMIT;
SELECT `mysql_1457` FROM `tbl_7`;
SELECT `mysql_1458` FROM `tbl_8`;
$dz$ dollar body 1459 ; semicolon inside $dz$
-- line 1460: deterministic comment
SELECT `mysql_1461` FROM `tbl_11`;
# hash comment 1462
INSERT INTO bench_t_55 (id, payload) VALUES (1463, 'O''Brien');
SELECT nested FROM t WHERE id IN (1464, 1465, 1466);
# hash comment 1465
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
WITH cte_1467 AS (SELECT 1467 AS n) SELECT n FROM cte_1467;
SELECT 1468 AS id, 'row_1468' AS label;
SELECT `mysql_1469` FROM `tbl_19`;
SELECT * FROM "quoted_1470" WHERE col = E'esc\'1470';
-- line 1471: deterministic comment
UPDATE bench_t_0 SET payload = 1472 WHERE id = 0;
SELECT [bracket_1473] FROM [dbo].[tbl_33];
BEGIN; SELECT 1474; COMMIT;
/* block header 1475 */
-- line 1476: deterministic comment
WITH cte_1477 AS (SELECT 1477 AS n) SELECT n FROM cte_1477;
$dz$ dollar body 1478 ; semicolon inside $dz$
SELECT nested FROM t WHERE id IN (1479, 1480, 1481);
$dz$ dollar body 1480 ; semicolon inside $dz$
INSERT INTO bench_t_73 (id, payload) VALUES (1481, 'v1481');
SELECT `mysql_1482` FROM `tbl_32`;
SELECT `mysql_1483` FROM `tbl_33`;
DELETE FROM bench_t_12 WHERE id = 12;
DELETE FROM bench_t_13 WHERE id = 13;
SELECT 1486 AS id, 'row_1486' AS label;
WITH cte_1487 AS (SELECT 1487 AS n) SELECT n FROM cte_1487;
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT nested FROM t WHERE id IN (1489, 1490, 1491);
UPDATE bench_t_18 SET payload = 1490 WHERE id = 18;
BEGIN; SELECT 1491; COMMIT;
SELECT * FROM "quoted_1492" WHERE col = E'esc\'1492';
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT nested FROM t WHERE id IN (1494, 1495, 1496);
DELETE FROM bench_t_23 WHERE id = 7;
SELECT `mysql_1496` FROM `tbl_46`;
SELECT [bracket_1497] FROM [dbo].[tbl_17];
SELECT `mysql_1498` FROM `tbl_48`;
INSERT INTO bench_t_91 (id, payload) VALUES (1499, 'v1499');
/*
 * section 6
 * checksum eefb
 */
DELETE FROM bench_t_28 WHERE id = 12;
SELECT 1505 AS id, 'row_1505' AS label;
SELECT 1506 AS id, 'row_1506' AS label;
-- line 1507: deterministic comment
BEGIN; SELECT 1508; COMMIT;
SELECT nested FROM t WHERE id IN (1509, 1510, 1511);
SELECT * FROM "quoted_1510" WHERE col = E'esc\'1510';
SELECT * FROM "quoted_1511" WHERE col = E'esc\'1511';
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
-- line 1513: deterministic comment
UPDATE bench_t_42 SET payload = 1514 WHERE id = 10;
/* block header 1515 */
INSERT INTO bench_t_108 (id, payload) VALUES (1516, 'v1516');
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT nested FROM t WHERE id IN (1518, 1519, 1520);
SELECT * FROM "quoted_1519" WHERE col = E'esc\'1519';
/* block header 1520 */
UPDATE bench_t_49 SET payload = 1521 WHERE id = 17;
SELECT 1522 AS id, 'row_1522' AS label;
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
$dz$ dollar body 1524 ; semicolon inside $dz$
SELECT * FROM "quoted_1525" WHERE col = E'esc\'1525';
UPDATE bench_t_54 SET payload = 1526 WHERE id = 22;
WITH cte_1527 AS (SELECT 1527 AS n) SELECT n FROM cte_1527;
SELECT * FROM "quoted_1528" WHERE col = E'esc\'1528';
$dz$ dollar body 1529 ; semicolon inside $dz$
SELECT 1530 AS id, 'row_1530' AS label;
SELECT * FROM "quoted_1531" WHERE col = E'esc\'1531';
UPDATE bench_t_60 SET payload = 1532 WHERE id = 28;
DELETE FROM bench_t_29 WHERE id = 13;
SELECT [bracket_1534] FROM [dbo].[tbl_14];
INSERT INTO bench_t_127 (id, payload) VALUES (1535, 'v1535');
$dz$ dollar body 1536 ; semicolon inside $dz$
# hash comment 1537
/* block header 1538 */
/* block header 1539 */
SELECT * FROM "quoted_1540" WHERE col = E'esc\'1540';
SELECT `mysql_1541` FROM `tbl_41`;
SELECT nested FROM t WHERE id IN (1542, 1543, 1544);
/* block header 1543 */
WITH cte_1544 AS (SELECT 1544 AS n) SELECT n FROM cte_1544;
SELECT nested FROM t WHERE id IN (1545, 1546, 1547);
$dz$ dollar body 1546 ; semicolon inside $dz$
SELECT * FROM "quoted_1547" WHERE col = E'esc\'1547';
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
BEGIN; SELECT 1549; COMMIT;
INSERT INTO bench_t_14 (id, payload) VALUES (1550, 'v1550');
SELECT [bracket_1551] FROM [dbo].[tbl_31];
$dz$ dollar body 1552 ; semicolon inside $dz$
$dz$ dollar body 1553 ; semicolon inside $dz$
$dz$ dollar body 1554 ; semicolon inside $dz$
# hash comment 1555
/* block header 1556 */
# hash comment 1557
/* block header 1558 */
$dz$ dollar body 1559 ; semicolon inside $dz$
SELECT * FROM "quoted_1560" WHERE col = E'esc\'1560';
SELECT [bracket_1561] FROM [dbo].[tbl_1];
WITH cte_1562 AS (SELECT 1562 AS n) SELECT n FROM cte_1562;
# hash comment 1563
INSERT INTO bench_t_28 (id, payload) VALUES (1564, 'v1564');
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
INSERT INTO bench_t_30 (id, payload) VALUES (1566, 'v1566');
SELECT 1567 AS id, 'row_1567' AS label;
SELECT `mysql_1568` FROM `tbl_18`;
UPDATE bench_t_33 SET payload = 1569 WHERE id = 1;
DELETE FROM bench_t_2 WHERE id = 2;
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
WITH cte_1572 AS (SELECT 1572 AS n) SELECT n FROM cte_1572;
DELETE FROM bench_t_5 WHERE id = 5;
SELECT `mysql_1574` FROM `tbl_24`;
SELECT nested FROM t WHERE id IN (1575, 1576, 1577);
SELECT 1576 AS id, 'row_1576' AS label;
WITH cte_1577 AS (SELECT 1577 AS n) SELECT n FROM cte_1577;
WITH cte_1578 AS (SELECT 1578 AS n) SELECT n FROM cte_1578;
INSERT INTO bench_t_43 (id, payload) VALUES (1579, 'v1579');
SELECT [bracket_1580] FROM [dbo].[tbl_20];
INSERT INTO bench_t_45 (id, payload) VALUES (1581, 'v1581');
# hash comment 1582
DELETE FROM bench_t_15 WHERE id = 15;
$dz$ dollar body 1584 ; semicolon inside $dz$
BEGIN; SELECT 1585; COMMIT;
/* block header 1586 */
BEGIN; SELECT 1587; COMMIT;
DELETE FROM bench_t_20 WHERE id = 4;
SELECT nested FROM t WHERE id IN (1589, 1590, 1591);
INSERT INTO bench_t_54 (id, payload) VALUES (1590, 'v1590');
DELETE FROM bench_t_23 WHERE id = 7;
# hash comment 1592
WITH cte_1593 AS (SELECT 1593 AS n) SELECT n FROM cte_1593;
-- line 1594: deterministic comment
SELECT [bracket_1595] FROM [dbo].[tbl_35];
SELECT * FROM "quoted_1596" WHERE col = E'esc\'1596';
BEGIN; SELECT 1597; COMMIT;
WITH cte_1598 AS (SELECT 1598 AS n) SELECT n FROM cte_1598;
DELETE FROM bench_t_31 WHERE id = 15;
UPDATE bench_t_0 SET payload = 1600 WHERE id = 0;
/* block header 1601 */
$dz$ dollar body 1602 ; semicolon inside $dz$
INSERT INTO bench_t_67 (id, payload) VALUES (1603, 'v1603');
SELECT * FROM "quoted_1604" WHERE col = E'esc\'1604';
INSERT INTO bench_t_69 (id, payload) VALUES (1605, 'v1605');
DELETE FROM bench_t_6 WHERE id = 6;
SELECT nested FROM t WHERE id IN (1607, 1608, 1609);
$dz$ dollar body 1608 ; semicolon inside $dz$
SELECT [bracket_1609] FROM [dbo].[tbl_9];
# hash comment 1610
-- line 1611: deterministic comment
WITH cte_1612 AS (SELECT 1612 AS n) SELECT n FROM cte_1612;
SELECT `mysql_1613` FROM `tbl_13`;
WITH cte_1614 AS (SELECT 1614 AS n) SELECT n FROM cte_1614;
SELECT `mysql_1615` FROM `tbl_15`;
/* block header 1616 */
DELETE FROM bench_t_17 WHERE id = 1;
DELETE FROM bench_t_18 WHERE id = 2;
/* block header 1619 */
SELECT nested FROM t WHERE id IN (1620, 1621, 1622);
SELECT * FROM "quoted_1621" WHERE col = E'esc\'1621';
WITH cte_1622 AS (SELECT 1622 AS n) SELECT n FROM cte_1622;
$dz$ dollar body 1623 ; semicolon inside $dz$
SELECT * FROM "quoted_1624" WHERE col = E'esc\'1624';
SELECT * FROM "quoted_1625" WHERE col = E'esc\'1625';
DELETE FROM bench_t_26 WHERE id = 10;
-- line 1627: deterministic comment
UPDATE bench_t_28 SET payload = 1628 WHERE id = 28;
-- line 1629: deterministic comment
SELECT 1630 AS id, 'row_1630' AS label;
UPDATE bench_t_31 SET payload = 1631 WHERE id = 31;
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
INSERT INTO bench_t_97 (id, payload) VALUES (1633, 'v1633');
BEGIN; SELECT 1634; COMMIT;
BEGIN; SELECT 1635; COMMIT;
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
INSERT INTO bench_t_101 (id, payload) VALUES (1637, 'v1637');
SELECT nested FROM t WHERE id IN (1638, 1639, 1640);
WITH cte_1639 AS (SELECT 1639 AS n) SELECT n FROM cte_1639;
BEGIN; SELECT 1640; COMMIT;
SELECT nested FROM t WHERE id IN (1641, 1642, 1643);
SELECT nested FROM t WHERE id IN (1642, 1643, 1644);
SELECT nested FROM t WHERE id IN (1643, 1644, 1645);
DELETE FROM bench_t_12 WHERE id = 12;
SELECT * FROM "quoted_1645" WHERE col = E'esc\'1645';
SELECT [bracket_1646] FROM [dbo].[tbl_6];
DELETE FROM bench_t_15 WHERE id = 15;
BEGIN; SELECT 1648; COMMIT;
SELECT nested FROM t WHERE id IN (1649, 1650, 1651);
-- line 1650: deterministic comment
BEGIN; SELECT 1651; COMMIT;
BEGIN; SELECT 1652; COMMIT;
DELETE FROM bench_t_21 WHERE id = 5;
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
DELETE FROM bench_t_23 WHERE id = 7;
BEGIN; SELECT 1656; COMMIT;
SELECT 1657 AS id, 'row_1657' AS label;
$dz$ dollar body 1658 ; semicolon inside $dz$
$dz$ dollar body 1659 ; semicolon inside $dz$
UPDATE bench_t_60 SET payload = 1660 WHERE id = 28;
BEGIN; SELECT 1661; COMMIT;
DELETE FROM bench_t_30 WHERE id = 14;
/* block header 1663 */
# hash comment 1664
UPDATE bench_t_1 SET payload = 1665 WHERE id = 1;
SELECT * FROM "quoted_1666" WHERE col = E'esc\'1666';
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT [bracket_1668] FROM [dbo].[tbl_28];
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
# hash comment 1670
SELECT * FROM "quoted_1671" WHERE col = E'esc\'1671';
DELETE FROM bench_t_8 WHERE id = 8;
SELECT 1673 AS id, 'row_1673' AS label;
/* block header 1674 */
WITH cte_1675 AS (SELECT 1675 AS n) SELECT n FROM cte_1675;
# hash comment 1676
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT 1678 AS id, 'row_1678' AS label;
$dz$ dollar body 1679 ; semicolon inside $dz$
SELECT nested FROM t WHERE id IN (1680, 1681, 1682);
SELECT [bracket_1681] FROM [dbo].[tbl_1];
SELECT 1682 AS id, 'row_1682' AS label;
SELECT nested FROM t WHERE id IN (1683, 1684, 1685);
SELECT [bracket_1684] FROM [dbo].[tbl_4];
-- line 1685: deterministic comment
BEGIN; SELECT 1686; COMMIT;
UPDATE bench_t_23 SET payload = 1687 WHERE id = 23;
-- line 1688: deterministic comment
UPDATE bench_t_25 SET payload = 1689 WHERE id = 25;
/* block header 1690 */
/* block header 1691 */
-- line 1692: deterministic comment
SELECT * FROM "quoted_1693" WHERE col = E'esc\'1693';
/* block header 1694 */
INSERT INTO bench_t_31 (id, payload) VALUES (1695, 'v1695');
BEGIN; SELECT 1696; COMMIT;
UPDATE bench_t_33 SET payload = 1697 WHERE id = 1;
/* block header 1698 */
SELECT [bracket_1699] FROM [dbo].[tbl_19];
UPDATE bench_t_36 SET payload = 1700 WHERE id = 4;
$dz$ dollar body 1701 ; semicolon inside $dz$
-- line 1702: deterministic comment
$dz$ dollar body 1703 ; semicolon inside $dz$
/* block header 1704 */
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
$dz$ dollar body 1706 ; semicolon inside $dz$
-- line 1707: deterministic comment
SELECT `mysql_1708` FROM `tbl_8`;
# hash comment 1709
$dz$ dollar body 1710 ; semicolon inside $dz$
$dz$ dollar body 1711 ; semicolon inside $dz$
SELECT * FROM "quoted_1712" WHERE col = E'esc\'1712';
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
/* block header 1714 */
$dz$ dollar body 1715 ; semicolon inside $dz$
/* block header 1716 */
INSERT INTO bench_t_53 (id, payload) VALUES (1717, 'v1717');
INSERT INTO bench_t_54 (id, payload) VALUES (1718, 'v1718');
UPDATE bench_t_55 SET payload = 1719 WHERE id = 23;
/* block header 1720 */
SELECT * FROM "quoted_1721" WHERE col = E'esc\'1721';
# hash comment 1722
DELETE FROM bench_t_27 WHERE id = 11;
UPDATE bench_t_60 SET payload = 1724 WHERE id = 28;
BEGIN; SELECT 1725; COMMIT;
SELECT 1726 AS id, 'row_1726' AS label;
$dz$ dollar body 1727 ; semicolon inside $dz$
BEGIN; SELECT 1728; COMMIT;
UPDATE bench_t_1 SET payload = 1729 WHERE id = 1;
SELECT 1730 AS id, 'row_1730' AS label;
SELECT * FROM "quoted_1731" WHERE col = E'esc\'1731';
-- line 1732: deterministic comment
BEGIN; SELECT 1733; COMMIT;
# hash comment 1734
INSERT INTO bench_t_71 (id, payload) VALUES (1735, 'v1735');
$dz$ dollar body 1736 ; semicolon inside $dz$
DELETE FROM bench_t_9 WHERE id = 9;
SELECT [bracket_1738] FROM [dbo].[tbl_18];
-- line 1739: deterministic comment
-- line 1740: deterministic comment
SELECT nested FROM t WHERE id IN (1741, 1742, 1743);
/* block header 1742 */
# hash comment 1743
SELECT 1744 AS id, 'row_1744' AS label;
WITH cte_1745 AS (SELECT 1745 AS n) SELECT n FROM cte_1745;
INSERT INTO bench_t_82 (id, payload) VALUES (1746, 'v1746');
UPDATE bench_t_19 SET payload = 1747 WHERE id = 19;
BEGIN; SELECT 1748; COMMIT;
# hash comment 1749
/*
 * section 7
 * checksum 5b57
 */
SELECT [bracket_1750] FROM [dbo].[tbl_30];
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
UPDATE bench_t_28 SET payload = 1756 WHERE id = 28;
-- line 1757: deterministic comment
SELECT `mysql_1758` FROM `tbl_8`;
BEGIN; SELECT 1759; COMMIT;
/* block header 1760 */
/* block header 1761 */
SELECT [bracket_1762] FROM [dbo].[tbl_2];
SELECT nested FROM t WHERE id IN (1763, 1764, 1765);
DELETE FROM bench_t_4 WHERE id = 4;
SELECT * FROM "quoted_1765" WHERE col = E'esc\'1765';
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT [bracket_1767] FROM [dbo].[tbl_7];
SELECT 1768 AS id, 'row_1768' AS label;
SELECT [bracket_1769] FROM [dbo].[tbl_9];
$dz$ dollar body 1770 ; semicolon inside $dz$
SELECT nested FROM t WHERE id IN (1771, 1772, 1773);
WITH cte_1772 AS (SELECT 1772 AS n) SELECT n FROM cte_1772;
BEGIN; SELECT 1773; COMMIT;
WITH cte_1774 AS (SELECT 1774 AS n) SELECT n FROM cte_1774;
SELECT 1775 AS id, 'row_1775' AS label;
-- line 1776: deterministic comment
SELECT [bracket_1777] FROM [dbo].[tbl_17];
SELECT [bracket_1778] FROM [dbo].[tbl_18];
INSERT INTO bench_t_115 (id, payload) VALUES (1779, 'v1779');
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT 1781 AS id, 'row_1781' AS label;
-- line 1782: deterministic comment
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
UPDATE bench_t_56 SET payload = 1784 WHERE id = 24;
DELETE FROM bench_t_25 WHERE id = 9;
-- line 1786: deterministic comment
SELECT 1787 AS id, 'row_1787' AS label;
DELETE FROM bench_t_28 WHERE id = 12;
WITH cte_1789 AS (SELECT 1789 AS n) SELECT n FROM cte_1789;
WITH cte_1790 AS (SELECT 1790 AS n) SELECT n FROM cte_1790;
SELECT 1791 AS id, 'row_1791' AS label;
-- line 1792: deterministic comment
SELECT [bracket_1793] FROM [dbo].[tbl_33];
SELECT 1794 AS id, 'row_1794' AS label;
SELECT `mysql_1795` FROM `tbl_45`;
WITH cte_1796 AS (SELECT 1796 AS n) SELECT n FROM cte_1796;
INSERT INTO bench_t_5 (id, payload) VALUES (1797, 'v1797');
-- line 1798: deterministic comment
INSERT INTO bench_t_7 (id, payload) VALUES (1799, 'v1799');
SELECT nested FROM t WHERE id IN (1800, 1801, 1802);
SELECT 1801 AS id, 'row_1801' AS label;
# hash comment 1802
# hash comment 1803
DELETE FROM bench_t_12 WHERE id = 12;
INSERT INTO bench_t_13 (id, payload) VALUES (1805, 'v1805');
$dz$ dollar body 1806 ; semicolon inside $dz$
SELECT [bracket_1807] FROM [dbo].[tbl_7];
SELECT 1808 AS id, 'row_1808' AS label;
SELECT [bracket_1809] FROM [dbo].[tbl_9];
INSERT INTO bench_t_18 (id, payload) VALUES (1810, 'v1810');
INSERT INTO bench_t_19 (id, payload) VALUES (1811, 'v1811');
SELECT * FROM "quoted_1812" WHERE col = E'esc\'1812';
SELECT [bracket_1813] FROM [dbo].[tbl_13];
/* block header 1814 */
-- line 1815: deterministic comment
INSERT INTO bench_t_24 (id, payload) VALUES (1816, 'v1816');
UPDATE bench_t_25 SET payload = 1817 WHERE id = 25;
BEGIN; SELECT 1818; COMMIT;
UPDATE bench_t_27 SET payload = 1819 WHERE id = 27;
WITH cte_1820 AS (SELECT 1820 AS n) SELECT n FROM cte_1820;
UPDATE bench_t_29 SET payload = 1821 WHERE id = 29;
$dz$ dollar body 1822 ; semicolon inside $dz$
# hash comment 1823
# hash comment 1824
BEGIN; SELECT 1825; COMMIT;
INSERT INTO bench_t_34 (id, payload) VALUES (1826, 'O''Brien');
$dz$ dollar body 1827 ; semicolon inside $dz$
SELECT * FROM "quoted_1828" WHERE col = E'esc\'1828';
SELECT [bracket_1829] FROM [dbo].[tbl_29];
-- line 1830: deterministic comment
SELECT 1831 AS id, 'row_1831' AS label;
DELETE FROM bench_t_8 WHERE id = 8;
DELETE FROM bench_t_9 WHERE id = 9;
SELECT 1834 AS id, 'row_1834' AS label;
DELETE FROM bench_t_11 WHERE id = 11;
SELECT * FROM "quoted_1836" WHERE col = E'esc\'1836';
UPDATE bench_t_45 SET payload = 1837 WHERE id = 13;
UPDATE bench_t_46 SET payload = 1838 WHERE id = 14;
SELECT * FROM "quoted_1839" WHERE col = E'esc\'1839';
SELECT nested FROM t WHERE id IN (1840, 1841, 1842);
SELECT * FROM "quoted_1841" WHERE col = E'esc\'1841';
BEGIN; SELECT 1842; COMMIT;
SELECT 1843 AS id, 'row_1843' AS label;
INSERT INTO bench_t_52 (id, payload) VALUES (1844, 'v1844');
SELECT [bracket_1845] FROM [dbo].[tbl_5];
UPDATE bench_t_54 SET payload = 1846 WHERE id = 22;
SELECT nested FROM t WHERE id IN (1847, 1848, 1849);
WITH cte_1848 AS (SELECT 1848 AS n) SELECT n FROM cte_1848;
INSERT INTO bench_t_57 (id, payload) VALUES (1849, 'v1849');
SELECT [bracket_1850] FROM [dbo].[tbl_10];
SELECT [bracket_1851] FROM [dbo].[tbl_11];
BEGIN; SELECT 1852; COMMIT;
$dz$ dollar body 1853 ; semicolon inside $dz$
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
$dz$ dollar body 1855 ; semicolon inside $dz$
UPDATE bench_t_0 SET payload = 1856 WHERE id = 0;
SELECT nested FROM t WHERE id IN (1857, 1858, 1859);
BEGIN; SELECT 1858; COMMIT;
WITH cte_1859 AS (SELECT 1859 AS n) SELECT n FROM cte_1859;
BEGIN; SELECT 1860; COMMIT;
$dz$ dollar body 1861 ; semicolon inside $dz$
$dz$ dollar body 1862 ; semicolon inside $dz$
# hash comment 1863
SELECT nested FROM t WHERE id IN (1864, 1865, 1866);
WITH cte_1865 AS (SELECT 1865 AS n) SELECT n FROM cte_1865;
INSERT INTO bench_t_74 (id, payload) VALUES (1866, 'v1866');
INSERT INTO bench_t_75 (id, payload) VALUES (1867, 'v1867');
SELECT `mysql_1868` FROM `tbl_18`;
SELECT [bracket_1869] FROM [dbo].[tbl_29];
$dz$ dollar body 1870 ; semicolon inside $dz$
SELECT nested FROM t WHERE id IN (1871, 1872, 1873);
SELECT nested FROM t WHERE id IN (1872, 1873, 1874);
SELECT * FROM "quoted_1873" WHERE col = E'esc\'1873';
SELECT 1874 AS id, 'row_1874' AS label;
-- line 1875: deterministic comment
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
BEGIN; SELECT 1877; COMMIT;
UPDATE bench_t_22 SET payload = 1878 WHERE id = 22;
-- line 1879: deterministic comment
SELECT nested FROM t WHERE id IN (1880, 1881, 1882);
DELETE FROM bench_t_25 WHERE id = 9;
/* block header 1882 */
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
UPDATE bench_t_28 SET payload = 1884 WHERE id = 28;
UPDATE bench_t_29 SET payload = 1885 WHERE id = 29;
DELETE FROM bench_t_30 WHERE id = 14;
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
UPDATE bench_t_32 SET payload = 1888 WHERE id = 0;
SELECT [bracket_1889] FROM [dbo].[tbl_9];
UPDATE bench_t_34 SET payload = 1890 WHERE id = 2;
SELECT * FROM "quoted_1891" WHERE col = E'esc\'1891';
WITH cte_1892 AS (SELECT 1892 AS n) SELECT n FROM cte_1892;
$dz$ dollar body 1893 ; semicolon inside $dz$
SELECT 1894 AS id, 'row_1894' AS label;
WITH cte_1895 AS (SELECT 1895 AS n) SELECT n FROM cte_1895;
UPDATE bench_t_40 SET payload = 1896 WHERE id = 8;
$dz$ dollar body 1897 ; semicolon inside $dz$
SELECT `mysql_1898` FROM `tbl_48`;
/* block header 1899 */
# hash comment 1900
-- line 1901: deterministic comment
SELECT * FROM "quoted_1902" WHERE col = E'esc\'1902';
/* block header 1903 */
SELECT `mysql_1904` FROM `tbl_4`;
DELETE FROM bench_t_17 WHERE id = 1;
WITH cte_1906 AS (SELECT 1906 AS n) SELECT n FROM cte_1906;
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
WITH cte_1908 AS (SELECT 1908 AS n) SELECT n FROM cte_1908;
WITH cte_1909 AS (SELECT 1909 AS n) SELECT n FROM cte_1909;
SELECT 1910 AS id, 'row_1910' AS label;
SELECT nested FROM t WHERE id IN (1911, 1912, 1913);
$dz$ dollar body 1912 ; semicolon inside $dz$
-- line 1913: deterministic comment
SELECT * FROM "quoted_1914" WHERE col = E'esc\'1914';
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT * FROM "quoted_1916" WHERE col = E'esc\'1916';
/* block header 1917 */
SELECT 1918 AS id, 'row_1918' AS label;
BEGIN; SELECT 1919; COMMIT;
INSERT INTO bench_t_0 (id, payload) VALUES (1920, 'v1920');
INSERT INTO bench_t_1 (id, payload) VALUES (1921, 'v1921');
SELECT 1922 AS id, 'row_1922' AS label;
BEGIN; SELECT 1923; COMMIT;
/* block header 1924 */
SELECT [bracket_1925] FROM [dbo].[tbl_5];
WITH cte_1926 AS (SELECT 1926 AS n) SELECT n FROM cte_1926;
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
$dz$ dollar body 1928 ; semicolon inside $dz$
BEGIN; SELECT 1929; COMMIT;
$dz$ dollar body 1930 ; semicolon inside $dz$
WITH cte_1931 AS (SELECT 1931 AS n) SELECT n FROM cte_1931;
INSERT INTO bench_t_12 (id, payload) VALUES (1932, 'v1932');
INSERT INTO bench_t_13 (id, payload) VALUES (1933, 'v1933');
/* block header 1934 */
WITH cte_1935 AS (SELECT 1935 AS n) SELECT n FROM cte_1935;
BEGIN; SELECT 1936; COMMIT;
-- line 1937: deterministic comment
-- line 1938: deterministic comment
SELECT * FROM "quoted_1939" WHERE col = E'esc\'1939';
# hash comment 1940
WITH cte_1941 AS (SELECT 1941 AS n) SELECT n FROM cte_1941;
BEGIN; SELECT 1942; COMMIT;
SELECT * FROM "quoted_1943" WHERE col = E'esc\'1943';
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
BEGIN; SELECT 1945; COMMIT;
SELECT `mysql_1946` FROM `tbl_46`;
DELETE FROM bench_t_27 WHERE id = 11;
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
# hash comment 1949
WITH cte_1950 AS (SELECT 1950 AS n) SELECT n FROM cte_1950;
BEGIN; SELECT 1951; COMMIT;
SELECT [bracket_1952] FROM [dbo].[tbl_32];
SELECT `mysql_1953` FROM `tbl_3`;
INSERT INTO bench_t_34 (id, payload) VALUES (1954, 'v1954');
# hash comment 1955
# hash comment 1956
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT nested FROM t WHERE id IN (1958, 1959, 1960);
SELECT `mysql_1959` FROM `tbl_9`;
UPDATE bench_t_40 SET payload = 1960 WHERE id = 8;
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
WITH cte_1962 AS (SELECT 1962 AS n) SELECT n FROM cte_1962;
/* block header 1963 */
SELECT 1964 AS id, 'row_1964' AS label;
-- line 1965: deterministic comment
-- line 1966: deterministic comment
-- line 1967: deterministic comment
DELETE FROM bench_t_16 WHERE id = 0;
# hash comment 1969
-- line 1970: deterministic comment
SELECT * FROM "quoted_1971" WHERE col = E'esc\'1971';
BEGIN; SELECT 1972; COMMIT;
SELECT nested FROM t WHERE id IN (1973, 1974, 1975);
/* block header 1974 */
SELECT * FROM "quoted_1975" WHERE col = E'esc\'1975';
$dz$ dollar body 1976 ; semicolon inside $dz$
/* block header 1977 */
SELECT 1978 AS id, 'row_1978' AS label;
SELECT 1979 AS id, 'row_1979' AS label;
INSERT INTO bench_t_60 (id, payload) VALUES (1980, 'O''Brien');
INSERT INTO bench_t_61 (id, payload) VALUES (1981, 'v1981');
DELETE FROM bench_t_30 WHERE id = 14;
SELECT [bracket_1983] FROM [dbo].[tbl_23];
SELECT `mysql_1984` FROM `tbl_34`;
SELECT 1985 AS id, 'row_1985' AS label;
SELECT `mysql_1986` FROM `tbl_36`;
BEGIN; SELECT 1987; COMMIT;
UPDATE bench_t_4 SET payload = 1988 WHERE id = 4;
SELECT * FROM "quoted_1989" WHERE col = E'esc\'1989';
$dz$ dollar body 1990 ; semicolon inside $dz$
INSERT INTO bench_t_71 (id, payload) VALUES (1991, 'O''Brien');
SELECT nested FROM t WHERE id IN (1992, 1993, 1994);
DELETE FROM bench_t_9 WHERE id = 9;
DELETE FROM bench_t_10 WHERE id = 10;
-- line 1995: deterministic comment
DELETE FROM bench_t_12 WHERE id = 12;
SELECT 1997 AS id, 'row_1997' AS label;
# hash comment 1998
BEGIN; SELECT 1999; COMMIT;
/*
 * section 8
 * checksum 8205
 */
SELECT * FROM "quoted_2000" WHERE col = E'esc\'2000';
# hash comment 2005
WITH cte_2006 AS (SELECT 2006 AS n) SELECT n FROM cte_2006;
SELECT 2007 AS id, 'row_2007' AS label;
INSERT INTO bench_t_88 (id, payload) VALUES (2008, 'v2008');
SELECT * FROM "quoted_2009" WHERE col = E'esc\'2009';
-- line 2010: deterministic comment
SELECT nested FROM t WHERE id IN (2011, 2012, 2013);
# hash comment 2012
INSERT INTO bench_t_93 (id, payload) VALUES (2013, 'O''Brien');
SELECT [bracket_2014] FROM [dbo].[tbl_14];
UPDATE bench_t_31 SET payload = 2015 WHERE id = 31;
SELECT nested FROM t WHERE id IN (2016, 2017, 2018);
-- line 2017: deterministic comment
SELECT nested FROM t WHERE id IN (2018, 2019, 2020);
-- line 2019: deterministic comment
UPDATE bench_t_36 SET payload = 2020 WHERE id = 4;
$dz$ dollar body 2021 ; semicolon inside $dz$
$dz$ dollar body 2022 ; semicolon inside $dz$
SELECT * FROM "quoted_2023" WHERE col = E'esc\'2023';
$dz$ dollar body 2024 ; semicolon inside $dz$
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
$dz$ dollar body 2027 ; semicolon inside $dz$
DELETE FROM bench_t_12 WHERE id = 12;
WITH cte_2029 AS (SELECT 2029 AS n) SELECT n FROM cte_2029;
SELECT * FROM "quoted_2030" WHERE col = E'esc\'2030';
$dz$ dollar body 2031 ; semicolon inside $dz$
DELETE FROM bench_t_16 WHERE id = 0;
SELECT 2033 AS id, 'row_2033' AS label;
INSERT INTO bench_t_114 (id, payload) VALUES (2034, 'v2034');
BEGIN; SELECT 2035; COMMIT;
SELECT `mysql_2036` FROM `tbl_36`;
DELETE FROM bench_t_21 WHERE id = 5;
SELECT * FROM "quoted_2038" WHERE col = E'esc\'2038';
INSERT INTO bench_t_119 (id, payload) VALUES (2039, 'v2039');
$dz$ dollar body 2040 ; semicolon inside $dz$
SELECT `mysql_2041` FROM `tbl_41`;
UPDATE bench_t_58 SET payload = 2042 WHERE id = 26;
SELECT [bracket_2043] FROM [dbo].[tbl_3];
SELECT * FROM "quoted_2044" WHERE col = E'esc\'2044';
SELECT * FROM "quoted_2045" WHERE col = E'esc\'2045';
/* block header 2046 */
/* block header 2047 */
SELECT nested FROM t WHERE id IN (2048, 2049, 2050);
SELECT * FROM "quoted_2049" WHERE col = E'esc\'2049';
SELECT 2050 AS id, 'row_2050' AS label;
-- line 2051: deterministic comment
INSERT INTO bench_t_4 (id, payload) VALUES (2052, 'v2052');
SELECT * FROM "quoted_2053" WHERE col = E'esc\'2053';
SELECT [bracket_2054] FROM [dbo].[tbl_14];
SELECT nested FROM t WHERE id IN (2055, 2056, 2057);
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT * FROM "quoted_2057" WHERE col = E'esc\'2057';
$dz$ dollar body 2058 ; semicolon inside $dz$
# hash comment 2059
WITH cte_2060 AS (SELECT 2060 AS n) SELECT n FROM cte_2060;
INSERT INTO bench_t_13 (id, payload) VALUES (2061, 'v2061');
SELECT `mysql_2062` FROM `tbl_12`;
-- line 2063: deterministic comment
SELECT * FROM "quoted_2064" WHERE col = E'esc\'2064';
INSERT INTO bench_t_17 (id, payload) VALUES (2065, 'v2065');
# hash comment 2066
SELECT * FROM "quoted_2067" WHERE col = E'esc\'2067';
SELECT 2068 AS id, 'row_2068' AS label;
SELECT [bracket_2069] FROM [dbo].[tbl_29];
UPDATE bench_t_22 SET payload = 2070 WHERE id = 22;
DELETE FROM bench_t_23 WHERE id = 7;
/* block header 2072 */
SELECT * FROM "quoted_2073" WHERE col = E'esc\'2073';
# hash comment 2074
# hash comment 2075
WITH cte_2076 AS (SELECT 2076 AS n) SELECT n FROM cte_2076;
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
/* block header 2078 */
UPDATE bench_t_31 SET payload = 2079 WHERE id = 31;
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT nested FROM t WHERE id IN (2081, 2082, 2083);
SELECT * FROM "quoted_2082" WHERE col = E'esc\'2082';
INSERT INTO bench_t_35 (id, payload) VALUES (2083, 'v2083');
# hash comment 2084
SELECT * FROM "quoted_2085" WHERE col = E'esc\'2085';
# hash comment 2086
$dz$ dollar body 2087 ; semicolon inside $dz$
BEGIN; SELECT 2088; COMMIT;
-- line 2089: deterministic comment
BEGIN; SELECT 2090; COMMIT;
INSERT INTO bench_t_43 (id, payload) VALUES (2091, 'v2091');
SELECT * FROM "quoted_2092" WHERE col = E'esc\'2092';
# hash comment 2093
INSERT INTO bench_t_46 (id, payload) VALUES (2094, 'v2094');
/* block header 2095 */
UPDATE bench_t_48 SET payload = 2096 WHERE id = 16;
/* block header 2097 */
BEGIN; SELECT 2098; COMMIT;
UPDATE bench_t_51 SET payload = 2099 WHERE id = 19;
DELETE FROM bench_t_20 WHERE id = 4;
DELETE FROM bench_t_21 WHERE id = 5;
DELETE FROM bench_t_22 WHERE id = 6;
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
# hash comment 2104
SELECT * FROM "quoted_2105" WHERE col = E'esc\'2105';
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
DELETE FROM bench_t_27 WHERE id = 11;
WITH cte_2108 AS (SELECT 2108 AS n) SELECT n FROM cte_2108;
$dz$ dollar body 2109 ; semicolon inside $dz$
# hash comment 2110
SELECT 2111 AS id, 'row_2111' AS label;
SELECT `mysql_2112` FROM `tbl_12`;
SELECT `mysql_2113` FROM `tbl_13`;
DELETE FROM bench_t_2 WHERE id = 2;
-- line 2115: deterministic comment
WITH cte_2116 AS (SELECT 2116 AS n) SELECT n FROM cte_2116;
UPDATE bench_t_5 SET payload = 2117 WHERE id = 5;
BEGIN; SELECT 2118; COMMIT;
SELECT `mysql_2119` FROM `tbl_19`;
SELECT `mysql_2120` FROM `tbl_20`;
SELECT 2121 AS id, 'row_2121' AS label;
$dz$ dollar body 2122 ; semicolon inside $dz$
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
-- line 2124: deterministic comment
BEGIN; SELECT 2125; COMMIT;
SELECT [bracket_2126] FROM [dbo].[tbl_6];
SELECT * FROM "quoted_2127" WHERE col = E'esc\'2127';
$dz$ dollar body 2128 ; semicolon inside $dz$
SELECT [bracket_2129] FROM [dbo].[tbl_9];
SELECT nested FROM t WHERE id IN (2130, 2131, 2132);
SELECT [bracket_2131] FROM [dbo].[tbl_11];
SELECT * FROM "quoted_2132" WHERE col = E'esc\'2132';
$dz$ dollar body 2133 ; semicolon inside $dz$
-- line 2134: deterministic comment
SELECT [bracket_2135] FROM [dbo].[tbl_15];
SELECT [bracket_2136] FROM [dbo].[tbl_16];
SELECT nested FROM t WHERE id IN (2137, 2138, 2139);
BEGIN; SELECT 2138; COMMIT;
/* block header 2139 */
WITH cte_2140 AS (SELECT 2140 AS n) SELECT n FROM cte_2140;
SELECT nested FROM t WHERE id IN (2141, 2142, 2143);
/* block header 2142 */
DELETE FROM bench_t_31 WHERE id = 15;
# hash comment 2144
-- line 2145: deterministic comment
SELECT [bracket_2146] FROM [dbo].[tbl_26];
SELECT nested FROM t WHERE id IN (2147, 2148, 2149);
UPDATE bench_t_36 SET payload = 2148 WHERE id = 4;
WITH cte_2149 AS (SELECT 2149 AS n) SELECT n FROM cte_2149;
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
$dz$ dollar body 2151 ; semicolon inside $dz$
WITH cte_2152 AS (SELECT 2152 AS n) SELECT n FROM cte_2152;
SELECT * FROM "quoted_2153" WHERE col = E'esc\'2153';
-- line 2154: deterministic comment
SELECT nested FROM t WHERE id IN (2155, 2156, 2157);
SELECT 2156 AS id, 'row_2156' AS label;
SELECT [bracket_2157] FROM [dbo].[tbl_37];
/* block header 2158 */
$dz$ dollar body 2159 ; semicolon inside $dz$
SELECT * FROM "quoted_2160" WHERE col = E'esc\'2160';
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
BEGIN; SELECT 2162; COMMIT;
SELECT * FROM "quoted_2163" WHERE col = E'esc\'2163';
SELECT `mysql_2164` FROM `tbl_14`;
SELECT 2165 AS id, 'row_2165' AS label;
-- line 2166: deterministic comment
SELECT 2167 AS id, 'row_2167' AS label;
SELECT `mysql_2168` FROM `tbl_18`;
SELECT * FROM "quoted_2169" WHERE col = E'esc\'2169';
-- line 2170: deterministic comment
SELECT * FROM "quoted_2171" WHERE col = E'esc\'2171';
UPDATE bench_t_60 SET payload = 2172 WHERE id = 28;
BEGIN; SELECT 2173; COMMIT;
INSERT INTO bench_t_126 (id, payload) VALUES (2174, 'v2174');
SELECT * FROM "quoted_2175" WHERE col = E'esc\'2175';
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
-- line 2177: deterministic comment
$dz$ dollar body 2178 ; semicolon inside $dz$
BEGIN; SELECT 2179; COMMIT;
WITH cte_2180 AS (SELECT 2180 AS n) SELECT n FROM cte_2180;
-- line 2181: deterministic comment
SELECT [bracket_2182] FROM [dbo].[tbl_22];
SELECT 2183 AS id, 'row_2183' AS label;
# hash comment 2184
BEGIN; SELECT 2185; COMMIT;
INSERT INTO bench_t_10 (id, payload) VALUES (2186, 'v2186');
UPDATE bench_t_11 SET payload = 2187 WHERE id = 11;
SELECT [bracket_2188] FROM [dbo].[tbl_28];
SELECT nested FROM t WHERE id IN (2189, 2190, 2191);
# hash comment 2190
/* block header 2191 */
DELETE FROM bench_t_16 WHERE id = 0;
SELECT nested FROM t WHERE id IN (2193, 2194, 2195);
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT [bracket_2195] FROM [dbo].[tbl_35];
UPDATE bench_t_20 SET payload = 2196 WHERE id = 20;
SELECT `mysql_2197` FROM `tbl_47`;
WITH cte_2198 AS (SELECT 2198 AS n) SELECT n FROM cte_2198;
# hash comment 2199
$dz$ dollar body 2200 ; semicolon inside $dz$
BEGIN; SELECT 2201; COMMIT;
-- line 2202: deterministic comment
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
INSERT INTO bench_t_28 (id, payload) VALUES (2204, 'v2204');
$dz$ dollar body 2205 ; semicolon inside $dz$
SELECT nested FROM t WHERE id IN (2206, 2207, 2208);
UPDATE bench_t_31 SET payload = 2207 WHERE id = 31;
SELECT nested FROM t WHERE id IN (2208, 2209, 2210);
$dz$ dollar body 2209 ; semicolon inside $dz$
SELECT [bracket_2210] FROM [dbo].[tbl_10];
INSERT INTO bench_t_35 (id, payload) VALUES (2211, 'O''Brien');
BEGIN; SELECT 2212; COMMIT;
# hash comment 2213
BEGIN; SELECT 2214; COMMIT;
-- line 2215: deterministic comment
/* block header 2216 */
BEGIN; SELECT 2217; COMMIT;
BEGIN; SELECT 2218; COMMIT;
BEGIN; SELECT 2219; COMMIT;
WITH cte_2220 AS (SELECT 2220 AS n) SELECT n FROM cte_2220;
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT 2222 AS id, 'row_2222' AS label;
SELECT 2223 AS id, 'row_2223' AS label;
SELECT `mysql_2224` FROM `tbl_24`;
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
BEGIN; SELECT 2226; COMMIT;
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT * FROM "quoted_2228" WHERE col = E'esc\'2228';
$dz$ dollar body 2229 ; semicolon inside $dz$
$dz$ dollar body 2230 ; semicolon inside $dz$
/* block header 2231 */
BEGIN; SELECT 2232; COMMIT;
DELETE FROM bench_t_25 WHERE id = 9;
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
/* block header 2235 */
$dz$ dollar body 2236 ; semicolon inside $dz$
SELECT 2237 AS id, 'row_2237' AS label;
# hash comment 2238
SELECT [bracket_2239] FROM [dbo].[tbl_39];
SELECT * FROM "quoted_2240" WHERE col = E'esc\'2240';
WITH cte_2241 AS (SELECT 2241 AS n) SELECT n FROM cte_2241;
UPDATE bench_t_2 SET payload = 2242 WHERE id = 2;
SELECT [bracket_2243] FROM [dbo].[tbl_3];
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT [bracket_2245] FROM [dbo].[tbl_5];
SELECT 2246 AS id, 'row_2246' AS label;
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
-- line 2248: deterministic comment
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
/*
 * section 9
 * checksum 62a0
 */
$dz$ dollar body 2250 ; semicolon inside $dz$
SELECT nested FROM t WHERE id IN (2255, 2256, 2257);
SELECT * FROM "quoted_2256" WHERE col = E'esc\'2256';
DELETE FROM bench_t_17 WHERE id = 1;
-- line 2258: deterministic comment
SELECT `mysql_2259` FROM `tbl_9`;
# hash comment 2260
SELECT * FROM "quoted_2261" WHERE col = E'esc\'2261';
SELECT * FROM "quoted_2262" WHERE col = E'esc\'2262';
INSERT INTO bench_t_87 (id, payload) VALUES (2263, 'v2263');
SELECT 2264 AS id, 'row_2264' AS label;
WITH cte_2265 AS (SELECT 2265 AS n) SELECT n FROM cte_2265;
WITH cte_2266 AS (SELECT 2266 AS n) SELECT n FROM cte_2266;
WITH cte_2267 AS (SELECT 2267 AS n) SELECT n FROM cte_2267;
BEGIN; SELECT 2268; COMMIT;
WITH cte_2269 AS (SELECT 2269 AS n) SELECT n FROM cte_2269;
SELECT nested FROM t WHERE id IN (2270, 2271, 2272);
# hash comment 2271
/* block header 2272 */
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
UPDATE bench_t_34 SET payload = 2274 WHERE id = 2;
BEGIN; SELECT 2275; COMMIT;
SELECT nested FROM t WHERE id IN (2276, 2277, 2278);
BEGIN; SELECT 2277; COMMIT;
SELECT `mysql_2278` FROM `tbl_28`;
SELECT [bracket_2279] FROM [dbo].[tbl_39];
SELECT `mysql_2280` FROM `tbl_30`;
INSERT INTO bench_t_105 (id, payload) VALUES (2281, 'v2281');
SELECT [bracket_2282] FROM [dbo].[tbl_2];
# hash comment 2283
# hash comment 2284
SELECT 2285 AS id, 'row_2285' AS label;
# hash comment 2286
BEGIN; SELECT 2287; COMMIT;
SELECT `mysql_2288` FROM `tbl_38`;
-- line 2289: deterministic comment
SELECT 2290 AS id, 'row_2290' AS label;
SELECT [bracket_2291] FROM [dbo].[tbl_11];
UPDATE bench_t_52 SET payload = 2292 WHERE id = 20;
SELECT 2293 AS id, 'row_2293' AS label;
SELECT nested FROM t WHERE id IN (2294, 2295, 2296);
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT `mysql_2296` FROM `tbl_46`;
SELECT * FROM "quoted_2297" WHERE col = E'esc\'2297';
BEGIN; SELECT 2298; COMMIT;
SELECT nested FROM t WHERE id IN (2299, 2300, 2301);
/* block header 2300 */
SELECT nested FROM t WHERE id IN (2301, 2302, 2303);
SELECT * FROM "quoted_2302" WHERE col = E'esc\'2302';
SELECT * FROM "quoted_2303" WHERE col = E'esc\'2303';
SELECT nested FROM t WHERE id IN (2304, 2305, 2306);
/* block header 2305 */
INSERT INTO bench_t_2 (id, payload) VALUES (2306, 'v2306');
$dz$ dollar body 2307 ; semicolon inside $dz$
SELECT * FROM "quoted_2308" WHERE col = E'esc\'2308';
SELECT nested FROM t WHERE id IN (2309, 2310, 2311);
$dz$ dollar body 2310 ; semicolon inside $dz$
$dz$ dollar body 2311 ; semicolon inside $dz$
SELECT 2312 AS id, 'row_2312' AS label;
DELETE FROM bench_t_9 WHERE id = 9;
SELECT nested FROM t WHERE id IN (2314, 2315, 2316);
SELECT nested FROM t WHERE id IN (2315, 2316, 2317);
/* block header 2316 */
SELECT 2317 AS id, 'row_2317' AS label;
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
# hash comment 2319
SELECT 2320 AS id, 'row_2320' AS label;
UPDATE bench_t_17 SET payload = 2321 WHERE id = 17;
SELECT 2322 AS id, 'row_2322' AS label;
UPDATE bench_t_19 SET payload = 2323 WHERE id = 19;
-- line 2324: deterministic comment
/* block header 2325 */
DELETE FROM bench_t_22 WHERE id = 6;
-- line 2327: deterministic comment
SELECT * FROM "quoted_2328" WHERE col = E'esc\'2328';
SELECT * FROM "quoted_2329" WHERE col = E'esc\'2329';
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
$dz$ dollar body 2331 ; semicolon inside $dz$
SELECT [bracket_2332] FROM [dbo].[tbl_12];
-- line 2333: deterministic comment
# hash comment 2334
UPDATE bench_t_31 SET payload = 2335 WHERE id = 31;
BEGIN; SELECT 2336; COMMIT;
SELECT [bracket_2337] FROM [dbo].[tbl_17];
WITH cte_2338 AS (SELECT 2338 AS n) SELECT n FROM cte_2338;
/* block header 2339 */
SELECT * FROM "quoted_2340" WHERE col = E'esc\'2340';
SELECT `mysql_2341` FROM `tbl_41`;
UPDATE bench_t_38 SET payload = 2342 WHERE id = 6;
BEGIN; SELECT 2343; COMMIT;
SELECT [bracket_2344] FROM [dbo].[tbl_24];
$dz$ dollar body 2345 ; semicolon inside $dz$
INSERT INTO bench_t_42 (id, payload) VALUES (2346, 'v2346');
SELECT [bracket_2347] FROM [dbo].[tbl_27];
SELECT [bracket_2348] FROM [dbo].[tbl_28];
BEGIN; SELECT 2349; COMMIT;
UPDATE bench_t_46 SET payload = 2350 WHERE id = 14;
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
BEGIN; SELECT 2352; COMMIT;
DELETE FROM bench_t_17 WHERE id = 1;
UPDATE bench_t_50 SET payload = 2354 WHERE id = 18;
SELECT nested FROM t WHERE id IN (2355, 2356, 2357);
WITH cte_2356 AS (SELECT 2356 AS n) SELECT n FROM cte_2356;
WITH cte_2357 AS (SELECT 2357 AS n) SELECT n FROM cte_2357;
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT [bracket_2359] FROM [dbo].[tbl_39];
DELETE FROM bench_t_24 WHERE id = 8;
-- line 2361: deterministic comment
SELECT nested FROM t WHERE id IN (2362, 2363, 2364);
-- line 2363: deterministic comment
SELECT 2364 AS id, 'row_2364' AS label;
SELECT `mysql_2365` FROM `tbl_15`;
UPDATE bench_t_62 SET payload = 2366 WHERE id = 30;
WITH cte_2367 AS (SELECT 2367 AS n) SELECT n FROM cte_2367;
DELETE FROM bench_t_0 WHERE id = 0;
/* block header 2369 */
SELECT `mysql_2370` FROM `tbl_20`;
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT * FROM "quoted_2372" WHERE col = E'esc\'2372';
DELETE FROM bench_t_5 WHERE id = 5;
SELECT `mysql_2374` FROM `tbl_24`;
UPDATE bench_t_7 SET payload = 2375 WHERE id = 7;
INSERT INTO bench_t_72 (id, payload) VALUES (2376, 'O''Brien');
$dz$ dollar body 2377 ; semicolon inside $dz$
/* block header 2378 */
INSERT INTO bench_t_75 (id, payload) VALUES (2379, 'v2379');
UPDATE bench_t_12 SET payload = 2380 WHERE id = 12;
SELECT `mysql_2381` FROM `tbl_31`;
/* block header 2382 */
UPDATE bench_t_15 SET payload = 2383 WHERE id = 15;
BEGIN; SELECT 2384; COMMIT;
SELECT 2385 AS id, 'row_2385' AS label;
$dz$ dollar body 2386 ; semicolon inside $dz$
SELECT 2387 AS id, 'row_2387' AS label;
SELECT [bracket_2388] FROM [dbo].[tbl_28];
UPDATE bench_t_21 SET payload = 2389 WHERE id = 21;
SELECT nested FROM t WHERE id IN (2390, 2391, 2392);
# hash comment 2391
# hash comment 2392
$dz$ dollar body 2393 ; semicolon inside $dz$
/* block header 2394 */
INSERT INTO bench_t_91 (id, payload) VALUES (2395, 'v2395');
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
INSERT INTO bench_t_93 (id, payload) VALUES (2397, 'v2397');
DELETE FROM bench_t_30 WHERE id = 14;
# hash comment 2399
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
UPDATE bench_t_33 SET payload = 2401 WHERE id = 1;
-- line 2402: deterministic comment
/* block header 2403 */
-- line 2404: deterministic comment
INSERT INTO bench_t_101 (id, payload) VALUES (2405, 'v2405');
UPDATE bench_t_38 SET payload = 2406 WHERE id = 6;
INSERT INTO bench_t_103 (id, payload) VALUES (2407, 'v2407');
INSERT INTO bench_t_104 (id, payload) VALUES (2408, 'v2408');
DELETE FROM bench_t_9 WHERE id = 9;
UPDATE bench_t_42 SET payload = 2410 WHERE id = 10;
INSERT INTO bench_t_107 (id, payload) VALUES (2411, 'v2411');
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
UPDATE bench_t_45 SET payload = 2413 WHERE id = 13;
UPDATE bench_t_46 SET payload = 2414 WHERE id = 14;
UPDATE bench_t_47 SET payload = 2415 WHERE id = 15;
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT [bracket_2417] FROM [dbo].[tbl_17];
SELECT [bracket_2418] FROM [dbo].[tbl_18];
SELECT 2419 AS id, 'row_2419' AS label;
$dz$ dollar body 2420 ; semicolon inside $dz$
DELETE FROM bench_t_21 WHERE id = 5;
SELECT * FROM "quoted_2422" WHERE col = E'esc\'2422';
BEGIN; SELECT 2423; COMMIT;
INSERT INTO bench_t_120 (id, payload) VALUES (2424, 'v2424');
UPDATE bench_t_57 SET payload = 2425 WHERE id = 25;
SELECT `mysql_2426` FROM `tbl_26`;
SELECT * FROM "quoted_2427" WHERE col = E'esc\'2427';
# hash comment 2428
BEGIN; SELECT 2429; COMMIT;
-- line 2430: deterministic comment
SELECT [bracket_2431] FROM [dbo].[tbl_31];
BEGIN; SELECT 2432; COMMIT;
BEGIN; SELECT 2433; COMMIT;
SELECT `mysql_2434` FROM `tbl_34`;
SELECT * FROM "quoted_2435" WHERE col = E'esc\'2435';
/* block header 2436 */
SELECT nested FROM t WHERE id IN (2437, 2438, 2439);
SELECT [bracket_2438] FROM [dbo].[tbl_38];
-- line 2439: deterministic comment
WITH cte_2440 AS (SELECT 2440 AS n) SELECT n FROM cte_2440;
INSERT INTO bench_t_9 (id, payload) VALUES (2441, 'v2441');
SELECT * FROM "quoted_2442" WHERE col = E'esc\'2442';
/* block header 2443 */
SELECT [bracket_2444] FROM [dbo].[tbl_4];
$dz$ dollar body 2445 ; semicolon inside $dz$
-- line 2446: deterministic comment
WITH cte_2447 AS (SELECT 2447 AS n) SELECT n FROM cte_2447;
/* block header 2448 */
SELECT * FROM "quoted_2449" WHERE col = E'esc\'2449';
/* block header 2450 */
WITH cte_2451 AS (SELECT 2451 AS n) SELECT n FROM cte_2451;
WITH cte_2452 AS (SELECT 2452 AS n) SELECT n FROM cte_2452;
$dz$ dollar body 2453 ; semicolon inside $dz$
# hash comment 2454
SELECT nested FROM t WHERE id IN (2455, 2456, 2457);
BEGIN; SELECT 2456; COMMIT;
UPDATE bench_t_25 SET payload = 2457 WHERE id = 25;
BEGIN; SELECT 2458; COMMIT;
# hash comment 2459
$dz$ dollar body 2460 ; semicolon inside $dz$
# hash comment 2461
DELETE FROM bench_t_30 WHERE id = 14;
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT `mysql_2464` FROM `tbl_14`;
INSERT INTO bench_t_33 (id, payload) VALUES (2465, 'v2465');
UPDATE bench_t_34 SET payload = 2466 WHERE id = 2;
SELECT `mysql_2467` FROM `tbl_17`;
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT * FROM "quoted_2469" WHERE col = E'esc\'2469';
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
-- line 2473: deterministic comment
UPDATE bench_t_42 SET payload = 2474 WHERE id = 10;
INSERT INTO bench_t_43 (id, payload) VALUES (2475, 'O''Brien');
SELECT `mysql_2476` FROM `tbl_26`;
UPDATE bench_t_45 SET payload = 2477 WHERE id = 13;
-- line 2478: deterministic comment
SELECT nested FROM t WHERE id IN (2479, 2480, 2481);
$dz$ dollar body 2480 ; semicolon inside $dz$
SELECT `mysql_2481` FROM `tbl_31`;
SELECT 2482 AS id, 'row_2482' AS label;
BEGIN; SELECT 2483; COMMIT;
-- line 2484: deterministic comment
SELECT nested FROM t WHERE id IN (2485, 2486, 2487);
DELETE FROM bench_t_22 WHERE id = 6;
DELETE FROM bench_t_23 WHERE id = 7;
UPDATE bench_t_56 SET payload = 2488 WHERE id = 24;
SELECT * FROM "quoted_2489" WHERE col = E'esc\'2489';
$dz$ dollar body 2490 ; semicolon inside $dz$
SELECT [bracket_2491] FROM [dbo].[tbl_11];
BEGIN; SELECT 2492; COMMIT;
# hash comment 2493
SELECT 2494 AS id, 'row_2494' AS label;
$dz$ dollar body 2495 ; semicolon inside $dz$
INSERT INTO bench_t_64 (id, payload) VALUES (2496, 'v2496');
BEGIN; SELECT 2497; COMMIT;
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT `mysql_2499` FROM `tbl_49`;
/*
 * section 10
 * checksum 942d
 */
INSERT INTO bench_t_68 (id, payload) VALUES (2500, 'v2500');
SELECT `mysql_2505` FROM `tbl_5`;
INSERT INTO bench_t_74 (id, payload) VALUES (2506, 'v2506');
SELECT `mysql_2507` FROM `tbl_7`;
SELECT nested FROM t WHERE id IN (2508, 2509, 2510);
SELECT nested FROM t WHERE id IN (2509, 2510, 2511);
SELECT 2510 AS id, 'row_2510' AS label;
SELECT `mysql_2511` FROM `tbl_11`;
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
INSERT INTO bench_t_81 (id, payload) VALUES (2513, 'v2513');
SELECT `mysql_2514` FROM `tbl_14`;
-- line 2515: deterministic comment
UPDATE bench_t_20 SET payload = 2516 WHERE id = 20;
SELECT [bracket_2517] FROM [dbo].[tbl_37];
# hash comment 2518
# hash comment 2519
/* block header 2520 */
/* block header 2521 */
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT `mysql_2523` FROM `tbl_23`;
WITH cte_2524 AS (SELECT 2524 AS n) SELECT n FROM cte_2524;
$dz$ dollar body 2525 ; semicolon inside $dz$
# hash comment 2526
$dz$ dollar body 2527 ; semicolon inside $dz$
SELECT [bracket_2528] FROM [dbo].[tbl_8];
BEGIN; SELECT 2529; COMMIT;
$dz$ dollar body 2530 ; semicolon inside $dz$
SELECT `mysql_2531` FROM `tbl_31`;
SELECT nested FROM t WHERE id IN (2532, 2533, 2534);
SELECT [bracket_2533] FROM [dbo].[tbl_13];
UPDATE bench_t_38 SET payload = 2534 WHERE id = 6;
SELECT nested FROM t WHERE id IN (2535, 2536, 2537);
$dz$ dollar body 2536 ; semicolon inside $dz$
SELECT nested FROM t WHERE id IN (2537, 2538, 2539);
/* block header 2538 */
SELECT nested FROM t WHERE id IN (2539, 2540, 2541);
SELECT * FROM "quoted_2540" WHERE col = E'esc\'2540';
-- line 2541: deterministic comment
SELECT 2542 AS id, 'row_2542' AS label;
SELECT nested FROM t WHERE id IN (2543, 2544, 2545);
/* block header 2544 */
BEGIN; SELECT 2545; COMMIT;
UPDATE bench_t_50 SET payload = 2546 WHERE id = 18;
WITH cte_2547 AS (SELECT 2547 AS n) SELECT n FROM cte_2547;
UPDATE bench_t_52 SET payload = 2548 WHERE id = 20;
DELETE FROM bench_t_21 WHERE id = 5;
SELECT [bracket_2550] FROM [dbo].[tbl_30];
SELECT nested FROM t WHERE id IN (2551, 2552, 2553);
SELECT * FROM "quoted_2552" WHERE col = E'esc\'2552';
SELECT 2553 AS id, 'row_2553' AS label;
SELECT * FROM "quoted_2554" WHERE col = E'esc\'2554';
SELECT nested FROM t WHERE id IN (2555, 2556, 2557);
$dz$ dollar body 2556 ; semicolon inside $dz$
UPDATE bench_t_61 SET payload = 2557 WHERE id = 29;
# hash comment 2558
WITH cte_2559 AS (SELECT 2559 AS n) SELECT n FROM cte_2559;
SELECT 2560 AS id, 'row_2560' AS label;
UPDATE bench_t_1 SET payload = 2561 WHERE id = 1;
SELECT `mysql_2562` FROM `tbl_12`;
SELECT 2563 AS id, 'row_2563' AS label;
WITH cte_2564 AS (SELECT 2564 AS n) SELECT n FROM cte_2564;
-- line 2565: deterministic comment
-- line 2566: deterministic comment
$dz$ dollar body 2567 ; semicolon inside $dz$
-- line 2568: deterministic comment
INSERT INTO bench_t_9 (id, payload) VALUES (2569, 'v2569');
# hash comment 2570
BEGIN; SELECT 2571; COMMIT;
SELECT `mysql_2572` FROM `tbl_22`;
SELECT nested FROM t WHERE id IN (2573, 2574, 2575);
WITH cte_2574 AS (SELECT 2574 AS n) SELECT n FROM cte_2574;
-- line 2575: deterministic comment
UPDATE bench_t_16 SET payload = 2576 WHERE id = 16;
SELECT nested FROM t WHERE id IN (2577, 2578, 2579);
SELECT `mysql_2578` FROM `tbl_28`;
DELETE FROM bench_t_19 WHERE id = 3;
# hash comment 2580
SELECT nested FROM t WHERE id IN (2581, 2582, 2583);
# hash comment 2582
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
-- line 2584: deterministic comment
SELECT 2585 AS id, 'row_2585' AS label;
DELETE FROM bench_t_26 WHERE id = 10;
$dz$ dollar body 2587 ; semicolon inside $dz$
/* block header 2588 */
BEGIN; SELECT 2589; COMMIT;
SELECT nested FROM t WHERE id IN (2590, 2591, 2592);
DELETE FROM bench_t_31 WHERE id = 15;
DELETE FROM bench_t_0 WHERE id = 0;
$dz$ dollar body 2593 ; semicolon inside $dz$
INSERT INTO bench_t_34 (id, payload) VALUES (2594, 'v2594');
INSERT INTO bench_t_35 (id, payload) VALUES (2595, 'v2595');
-- line 2596: deterministic comment
SELECT 2597 AS id, 'row_2597' AS label;
WITH cte_2598 AS (SELECT 2598 AS n) SELECT n FROM cte_2598;
-- line 2599: deterministic comment
$dz$ dollar body 2600 ; semicolon inside $dz$
UPDATE bench_t_41 SET payload = 2601 WHERE id = 9;
DELETE FROM bench_t_10 WHERE id = 10;
SELECT `mysql_2603` FROM `tbl_3`;
SELECT `mysql_2604` FROM `tbl_4`;
SELECT [bracket_2605] FROM [dbo].[tbl_5];
SELECT * FROM "quoted_2606" WHERE col = E'esc\'2606';
SELECT `mysql_2607` FROM `tbl_7`;
SELECT [bracket_2608] FROM [dbo].[tbl_8];
SELECT nested FROM t WHERE id IN (2609, 2610, 2611);
DELETE FROM bench_t_18 WHERE id = 2;
INSERT INTO bench_t_51 (id, payload) VALUES (2611, 'v2611');
/* block header 2612 */
-- line 2613: deterministic comment
SELECT 2614 AS id, 'row_2614' AS label;
WITH cte_2615 AS (SELECT 2615 AS n) SELECT n FROM cte_2615;
INSERT INTO bench_t_56 (id, payload) VALUES (2616, 'v2616');
$dz$ dollar body 2617 ; semicolon inside $dz$
INSERT INTO bench_t_58 (id, payload) VALUES (2618, 'O''Brien');
# hash comment 2619
$dz$ dollar body 2620 ; semicolon inside $dz$
BEGIN; SELECT 2621; COMMIT;
/* block header 2622 */
/* block header 2623 */
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
/* block header 2626 */
$dz$ dollar body 2627 ; semicolon inside $dz$
/* block header 2628 */
UPDATE bench_t_5 SET payload = 2629 WHERE id = 5;
SELECT `mysql_2630` FROM `tbl_30`;
SELECT `mysql_2631` FROM `tbl_31`;
SELECT 2632 AS id, 'row_2632' AS label;
BEGIN; SELECT 2633; COMMIT;
WITH cte_2634 AS (SELECT 2634 AS n) SELECT n FROM cte_2634;
SELECT `mysql_2635` FROM `tbl_35`;
INSERT INTO bench_t_76 (id, payload) VALUES (2636, 'v2636');
/* block header 2637 */
# hash comment 2638
SELECT 2639 AS id, 'row_2639' AS label;
SELECT nested FROM t WHERE id IN (2640, 2641, 2642);
UPDATE bench_t_17 SET payload = 2641 WHERE id = 17;
WITH cte_2642 AS (SELECT 2642 AS n) SELECT n FROM cte_2642;
-- line 2643: deterministic comment
SELECT `mysql_2644` FROM `tbl_44`;
DELETE FROM bench_t_21 WHERE id = 5;
SELECT 2646 AS id, 'row_2646' AS label;
SELECT [bracket_2647] FROM [dbo].[tbl_7];
WITH cte_2648 AS (SELECT 2648 AS n) SELECT n FROM cte_2648;
/* block header 2649 */
$dz$ dollar body 2650 ; semicolon inside $dz$
$dz$ dollar body 2651 ; semicolon inside $dz$
SELECT [bracket_2652] FROM [dbo].[tbl_12];
# hash comment 2653
-- line 2654: deterministic comment
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT `mysql_2656` FROM `tbl_6`;
BEGIN; SELECT 2657; COMMIT;
# hash comment 2658
DELETE FROM bench_t_3 WHERE id = 3;
$dz$ dollar body 2660 ; semicolon inside $dz$
$dz$ dollar body 2661 ; semicolon inside $dz$
INSERT INTO bench_t_102 (id, payload) VALUES (2662, 'O''Brien');
-- line 2663: deterministic comment
DELETE FROM bench_t_8 WHERE id = 8;
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT * FROM "quoted_2666" WHERE col = E'esc\'2666';
WITH cte_2667 AS (SELECT 2667 AS n) SELECT n FROM cte_2667;
WITH cte_2668 AS (SELECT 2668 AS n) SELECT n FROM cte_2668;
-- line 2669: deterministic comment
DELETE FROM bench_t_14 WHERE id = 14;
BEGIN; SELECT 2671; COMMIT;
/* block header 2672 */
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
UPDATE bench_t_50 SET payload = 2674 WHERE id = 18;
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
BEGIN; SELECT 2677; COMMIT;
/* block header 2678 */
-- line 2679: deterministic comment
BEGIN; SELECT 2680; COMMIT;
BEGIN; SELECT 2681; COMMIT;
# hash comment 2682
SELECT 2683 AS id, 'row_2683' AS label;
/* block header 2684 */
SELECT 2685 AS id, 'row_2685' AS label;
$dz$ dollar body 2686 ; semicolon inside $dz$
BEGIN; SELECT 2687; COMMIT;
INSERT INTO bench_t_0 (id, payload) VALUES (2688, 'v2688');
WITH cte_2689 AS (SELECT 2689 AS n) SELECT n FROM cte_2689;
BEGIN; SELECT 2690; COMMIT;
/* block header 2691 */
SELECT `mysql_2692` FROM `tbl_42`;
# hash comment 2693
DELETE FROM bench_t_6 WHERE id = 6;
UPDATE bench_t_7 SET payload = 2695 WHERE id = 7;
UPDATE bench_t_8 SET payload = 2696 WHERE id = 8;
INSERT INTO bench_t_9 (id, payload) VALUES (2697, 'v2697');
SELECT 2698 AS id, 'row_2698' AS label;
BEGIN; SELECT 2699; COMMIT;
SELECT `mysql_2700` FROM `tbl_0`;
SELECT `mysql_2701` FROM `tbl_1`;
$dz$ dollar body 2702 ; semicolon inside $dz$
/* block header 2703 */
SELECT `mysql_2704` FROM `tbl_4`;
/* block header 2705 */
SELECT * FROM "quoted_2706" WHERE col = E'esc\'2706';
/* block header 2707 */
-- line 2708: deterministic comment
$dz$ dollar body 2709 ; semicolon inside $dz$
INSERT INTO bench_t_22 (id, payload) VALUES (2710, 'v2710');
DELETE FROM bench_t_23 WHERE id = 7;
SELECT nested FROM t WHERE id IN (2712, 2713, 2714);
SELECT 2713 AS id, 'row_2713' AS label;
BEGIN; SELECT 2714; COMMIT;
/* block header 2715 */
# hash comment 2716
SELECT `mysql_2717` FROM `tbl_17`;
SELECT * FROM "quoted_2718" WHERE col = E'esc\'2718';
BEGIN; SELECT 2719; COMMIT;
SELECT `mysql_2720` FROM `tbl_20`;
SELECT nested FROM t WHERE id IN (2721, 2722, 2723);
$dz$ dollar body 2722 ; semicolon inside $dz$
BEGIN; SELECT 2723; COMMIT;
BEGIN; SELECT 2724; COMMIT;
-- line 2725: deterministic comment
$dz$ dollar body 2726 ; semicolon inside $dz$
# hash comment 2727
/* block header 2728 */
SELECT * FROM "quoted_2729" WHERE col = E'esc\'2729';
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT nested FROM t WHERE id IN (2731, 2732, 2733);
SELECT `mysql_2732` FROM `tbl_32`;
BEGIN; SELECT 2733; COMMIT;
INSERT INTO bench_t_46 (id, payload) VALUES (2734, 'v2734');
UPDATE bench_t_47 SET payload = 2735 WHERE id = 15;
-- line 2736: deterministic comment
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT `mysql_2738` FROM `tbl_38`;
SELECT 2739 AS id, 'row_2739' AS label;
BEGIN; SELECT 2740; COMMIT;
SELECT * FROM "quoted_2741" WHERE col = E'esc\'2741';
$dz$ dollar body 2742 ; semicolon inside $dz$
WITH cte_2743 AS (SELECT 2743 AS n) SELECT n FROM cte_2743;
SELECT * FROM "quoted_2744" WHERE col = E'esc\'2744';
SELECT `mysql_2745` FROM `tbl_45`;
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT * FROM "quoted_2748" WHERE col = E'esc\'2748';
$dz$ dollar body 2749 ; semicolon inside $dz$
/*
 * section 11
 * checksum b7cd
 */
SELECT [bracket_2750] FROM [dbo].[tbl_30];
/* block header 2755 */
WITH cte_2756 AS (SELECT 2756 AS n) SELECT n FROM cte_2756;
/* block header 2757 */
UPDATE bench_t_6 SET payload = 2758 WHERE id = 6;
-- line 2759: deterministic comment
SELECT 2760 AS id, 'row_2760' AS label;
SELECT [bracket_2761] FROM [dbo].[tbl_1];
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT 2763 AS id, 'row_2763' AS label;
SELECT [bracket_2764] FROM [dbo].[tbl_4];
SELECT [bracket_2765] FROM [dbo].[tbl_5];
UPDATE bench_t_14 SET payload = 2766 WHERE id = 14;
BEGIN; SELECT 2767; COMMIT;
SELECT 2768 AS id, 'row_2768' AS label;
SELECT [bracket_2769] FROM [dbo].[tbl_9];
# hash comment 2770
UPDATE bench_t_19 SET payload = 2771 WHERE id = 19;
/* block header 2772 */
/* block header 2773 */
/* block header 2774 */
UPDATE bench_t_23 SET payload = 2775 WHERE id = 23;
DELETE FROM bench_t_24 WHERE id = 8;
INSERT INTO bench_t_89 (id, payload) VALUES (2777, 'v2777');
WITH cte_2778 AS (SELECT 2778 AS n) SELECT n FROM cte_2778;
SELECT [bracket_2779] FROM [dbo].[tbl_19];
BEGIN; SELECT 2780; COMMIT;
# hash comment 2781
SELECT nested FROM t WHERE id IN (2782, 2783, 2784);
/* block header 2783 */
DELETE FROM bench_t_0 WHERE id = 0;
SELECT [bracket_2785] FROM [dbo].[tbl_25];
DELETE FROM bench_t_2 WHERE id = 2;
-- line 2787: deterministic comment
$dz$ dollar body 2788 ; semicolon inside $dz$
INSERT INTO bench_t_101 (id, payload) VALUES (2789, 'v2789');
/* block header 2790 */
SELECT [bracket_2791] FROM [dbo].[tbl_31];
DELETE FROM bench_t_8 WHERE id = 8;
SELECT nested FROM t WHERE id IN (2793, 2794, 2795);
SELECT nested FROM t WHERE id IN (2794, 2795, 2796);
SELECT [bracket_2795] FROM [dbo].[tbl_35];
SELECT `mysql_2796` FROM `tbl_46`;
-- line 2797: deterministic comment
WITH cte_2798 AS (SELECT 2798 AS n) SELECT n FROM cte_2798;
$dz$ dollar body 2799 ; semicolon inside $dz$
UPDATE bench_t_48 SET payload = 2800 WHERE id = 16;
DELETE FROM bench_t_17 WHERE id = 1;
WITH cte_2802 AS (SELECT 2802 AS n) SELECT n FROM cte_2802;
SELECT [bracket_2803] FROM [dbo].[tbl_3];
DELETE FROM bench_t_20 WHERE id = 4;
SELECT [bracket_2805] FROM [dbo].[tbl_5];
UPDATE bench_t_54 SET payload = 2806 WHERE id = 22;
WITH cte_2807 AS (SELECT 2807 AS n) SELECT n FROM cte_2807;
# hash comment 2808
SELECT * FROM "quoted_2809" WHERE col = E'esc\'2809';
SELECT nested FROM t WHERE id IN (2810, 2811, 2812);
WITH cte_2811 AS (SELECT 2811 AS n) SELECT n FROM cte_2811;
BEGIN; SELECT 2812; COMMIT;
SELECT [bracket_2813] FROM [dbo].[tbl_13];
DELETE FROM bench_t_30 WHERE id = 14;
WITH cte_2815 AS (SELECT 2815 AS n) SELECT n FROM cte_2815;
SELECT [bracket_2816] FROM [dbo].[tbl_16];
SELECT [bracket_2817] FROM [dbo].[tbl_17];
BEGIN; SELECT 2818; COMMIT;
SELECT * FROM "quoted_2819" WHERE col = E'esc\'2819';
SELECT `mysql_2820` FROM `tbl_20`;
WITH cte_2821 AS (SELECT 2821 AS n) SELECT n FROM cte_2821;
BEGIN; SELECT 2822; COMMIT;
DELETE FROM bench_t_7 WHERE id = 7;
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
/* block header 2825 */
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT 2827 AS id, 'row_2827' AS label;
-- line 2828: deterministic comment
# hash comment 2829
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT `mysql_2831` FROM `tbl_31`;
UPDATE bench_t_16 SET payload = 2832 WHERE id = 16;
INSERT INTO bench_t_17 (id, payload) VALUES (2833, 'v2833');
# hash comment 2834
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT [bracket_2836] FROM [dbo].[tbl_36];
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT [bracket_2839] FROM [dbo].[tbl_39];
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
INSERT INTO bench_t_25 (id, payload) VALUES (2841, 'v2841');
DELETE FROM bench_t_26 WHERE id = 10;
-- line 2843: deterministic comment
BEGIN; SELECT 2844; COMMIT;
/* block header 2845 */
-- line 2846: deterministic comment
SELECT 2847 AS id, 'row_2847' AS label;
/* block header 2848 */
SELECT 2849 AS id, 'row_2849' AS label;
SELECT [bracket_2850] FROM [dbo].[tbl_10];
INSERT INTO bench_t_35 (id, payload) VALUES (2851, 'v2851');
SELECT `mysql_2852` FROM `tbl_2`;
SELECT [bracket_2853] FROM [dbo].[tbl_13];
SELECT `mysql_2854` FROM `tbl_4`;
/* block header 2855 */
BEGIN; SELECT 2856; COMMIT;
UPDATE bench_t_41 SET payload = 2857 WHERE id = 9;
$dz$ dollar body 2858 ; semicolon inside $dz$
UPDATE bench_t_43 SET payload = 2859 WHERE id = 11;
$dz$ dollar body 2860 ; semicolon inside $dz$
SELECT 2861 AS id, 'row_2861' AS label;
-- line 2862: deterministic comment
/* block header 2863 */
SELECT nested FROM t WHERE id IN (2864, 2865, 2866);
WITH cte_2865 AS (SELECT 2865 AS n) SELECT n FROM cte_2865;
$dz$ dollar body 2866 ; semicolon inside $dz$
UPDATE bench_t_51 SET payload = 2867 WHERE id = 19;
DELETE FROM bench_t_20 WHERE id = 4;
SELECT * FROM "quoted_2869" WHERE col = E'esc\'2869';
SELECT [bracket_2870] FROM [dbo].[tbl_30];
DELETE FROM bench_t_23 WHERE id = 7;
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
BEGIN; SELECT 2873; COMMIT;
BEGIN; SELECT 2874; COMMIT;
SELECT 2875 AS id, 'row_2875' AS label;
/* block header 2876 */
DELETE FROM bench_t_29 WHERE id = 13;
SELECT [bracket_2878] FROM [dbo].[tbl_38];
WITH cte_2879 AS (SELECT 2879 AS n) SELECT n FROM cte_2879;
UPDATE bench_t_0 SET payload = 2880 WHERE id = 0;
SELECT * FROM "quoted_2881" WHERE col = E'esc\'2881';
DELETE FROM bench_t_2 WHERE id = 2;
-- line 2883: deterministic comment
SELECT 2884 AS id, 'row_2884' AS label;
DELETE FROM bench_t_5 WHERE id = 5;
/* block header 2886 */
SELECT * FROM "quoted_2887" WHERE col = E'esc\'2887';
BEGIN; SELECT 2888; COMMIT;
$dz$ dollar body 2889 ; semicolon inside $dz$
SELECT * FROM "quoted_2890" WHERE col = E'esc\'2890';
/* block header 2891 */
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT nested FROM t WHERE id IN (2893, 2894, 2895);
BEGIN; SELECT 2894; COMMIT;
DELETE FROM bench_t_15 WHERE id = 15;
# hash comment 2896
SELECT * FROM "quoted_2897" WHERE col = E'esc\'2897';
DELETE FROM bench_t_18 WHERE id = 2;
SELECT `mysql_2899` FROM `tbl_49`;
$dz$ dollar body 2900 ; semicolon inside $dz$
INSERT INTO bench_t_85 (id, payload) VALUES (2901, 'v2901');
SELECT nested FROM t WHERE id IN (2902, 2903, 2904);
UPDATE bench_t_23 SET payload = 2903 WHERE id = 23;
-- line 2904: deterministic comment
$dz$ dollar body 2905 ; semicolon inside $dz$
UPDATE bench_t_26 SET payload = 2906 WHERE id = 26;
SELECT [bracket_2907] FROM [dbo].[tbl_27];
$dz$ dollar body 2908 ; semicolon inside $dz$
SELECT nested FROM t WHERE id IN (2909, 2910, 2911);
# hash comment 2910
UPDATE bench_t_31 SET payload = 2911 WHERE id = 31;
SELECT nested FROM t WHERE id IN (2912, 2913, 2914);
BEGIN; SELECT 2913; COMMIT;
$dz$ dollar body 2914 ; semicolon inside $dz$
/* block header 2915 */
/* block header 2916 */
WITH cte_2917 AS (SELECT 2917 AS n) SELECT n FROM cte_2917;
WITH cte_2918 AS (SELECT 2918 AS n) SELECT n FROM cte_2918;
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT `mysql_2920` FROM `tbl_20`;
INSERT INTO bench_t_105 (id, payload) VALUES (2921, 'v2921');
SELECT nested FROM t WHERE id IN (2922, 2923, 2924);
SELECT `mysql_2923` FROM `tbl_23`;
$dz$ dollar body 2924 ; semicolon inside $dz$
-- line 2925: deterministic comment
-- line 2926: deterministic comment
SELECT * FROM "quoted_2927" WHERE col = E'esc\'2927';
-- line 2928: deterministic comment
/* block header 2929 */
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
-- line 2931: deterministic comment
$dz$ dollar body 2932 ; semicolon inside $dz$
INSERT INTO bench_t_117 (id, payload) VALUES (2933, 'v2933');
SELECT [bracket_2934] FROM [dbo].[tbl_14];
SELECT `mysql_2935` FROM `tbl_35`;
INSERT INTO bench_t_120 (id, payload) VALUES (2936, 'v2936');
WITH cte_2937 AS (SELECT 2937 AS n) SELECT n FROM cte_2937;
SELECT [bracket_2938] FROM [dbo].[tbl_18];
INSERT INTO bench_t_123 (id, payload) VALUES (2939, 'v2939');
INSERT INTO bench_t_124 (id, payload) VALUES (2940, 'v2940');
INSERT INTO bench_t_125 (id, payload) VALUES (2941, 'v2941');
DELETE FROM bench_t_30 WHERE id = 14;
/* block header 2943 */
UPDATE bench_t_0 SET payload = 2944 WHERE id = 0;
/* block header 2945 */
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT `mysql_2947` FROM `tbl_47`;
SELECT nested FROM t WHERE id IN (2948, 2949, 2950);
SELECT * FROM "quoted_2949" WHERE col = E'esc\'2949';
DELETE FROM bench_t_6 WHERE id = 6;
DELETE FROM bench_t_7 WHERE id = 7;
# hash comment 2952
SELECT `mysql_2953` FROM `tbl_3`;
DELETE FROM bench_t_10 WHERE id = 10;
SELECT * FROM "quoted_2955" WHERE col = E'esc\'2955';
UPDATE bench_t_12 SET payload = 2956 WHERE id = 12;
-- line 2957: deterministic comment
SELECT [bracket_2958] FROM [dbo].[tbl_38];
SELECT nested FROM t WHERE id IN (2959, 2960, 2961);
SELECT [bracket_2960] FROM [dbo].[tbl_0];
SELECT * FROM "quoted_2961" WHERE col = E'esc\'2961';
WITH cte_2962 AS (SELECT 2962 AS n) SELECT n FROM cte_2962;
SELECT * FROM "quoted_2963" WHERE col = E'esc\'2963';
BEGIN; SELECT 2964; COMMIT;
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
WITH cte_2966 AS (SELECT 2966 AS n) SELECT n FROM cte_2966;
SELECT 2967 AS id, 'row_2967' AS label;
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
/* block header 2969 */
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
-- line 2971: deterministic comment
WITH cte_2972 AS (SELECT 2972 AS n) SELECT n FROM cte_2972;
SELECT `mysql_2973` FROM `tbl_23`;
SELECT nested FROM t WHERE id IN (2974, 2975, 2976);
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT * FROM "quoted_2977" WHERE col = E'esc\'2977';
SELECT [bracket_2978] FROM [dbo].[tbl_18];
SELECT `mysql_2979` FROM `tbl_29`;
$dz$ dollar body 2980 ; semicolon inside $dz$
SELECT nested FROM t WHERE id IN (2981, 2982, 2983);
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
$dz$ dollar body 2983 ; semicolon inside $dz$
# hash comment 2984
SELECT * FROM "quoted_2985" WHERE col = E'esc\'2985';
# hash comment 2986
UPDATE bench_t_43 SET payload = 2987 WHERE id = 11;
SELECT [bracket_2988] FROM [dbo].[tbl_28];
INSERT INTO bench_t_45 (id, payload) VALUES (2989, 'v2989');
-- line 2990: deterministic comment
SELECT nested FROM t WHERE id IN (2991, 2992, 2993);
DELETE FROM bench_t_16 WHERE id = 0;
-- line 2993: deterministic comment
/* block header 2994 */
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT `mysql_2996` FROM `tbl_46`;
SELECT * FROM "quoted_2997" WHERE col = E'esc\'2997';
BEGIN; SELECT 2998; COMMIT;
BEGIN; SELECT 2999; COMMIT;
/*
 * section 12
 * checksum c082
 */
SELECT * FROM "quoted_3000" WHERE col = E'esc\'3000';
SELECT [bracket_3005] FROM [dbo].[tbl_5];
-- line 3006: deterministic comment
SELECT [bracket_3007] FROM [dbo].[tbl_7];
$dz$ dollar body 3008 ; semicolon inside $dz$
SELECT nested FROM t WHERE id IN (3009, 3010, 3011);
/* block header 3010 */
SELECT [bracket_3011] FROM [dbo].[tbl_11];
INSERT INTO bench_t_68 (id, payload) VALUES (3012, 'v3012');
SELECT nested FROM t WHERE id IN (3013, 3014, 3015);
UPDATE bench_t_6 SET payload = 3014 WHERE id = 6;
UPDATE bench_t_7 SET payload = 3015 WHERE id = 7;
SELECT * FROM "quoted_3016" WHERE col = E'esc\'3016';
-- line 3017: deterministic comment
SELECT [bracket_3018] FROM [dbo].[tbl_18];
SELECT nested FROM t WHERE id IN (3019, 3020, 3021);
$dz$ dollar body 3020 ; semicolon inside $dz$
SELECT 3021 AS id, 'row_3021' AS label;
DELETE FROM bench_t_14 WHERE id = 14;
/* block header 3023 */
WITH cte_3024 AS (SELECT 3024 AS n) SELECT n FROM cte_3024;
$dz$ dollar body 3025 ; semicolon inside $dz$
# hash comment 3026
SELECT [bracket_3027] FROM [dbo].[tbl_27];
SELECT [bracket_3028] FROM [dbo].[tbl_28];
SELECT * FROM "quoted_3029" WHERE col = E'esc\'3029';
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT * FROM "quoted_3031" WHERE col = E'esc\'3031';
-- line 3032: deterministic comment
SELECT [bracket_3033] FROM [dbo].[tbl_33];
BEGIN; SELECT 3034; COMMIT;
WITH cte_3035 AS (SELECT 3035 AS n) SELECT n FROM cte_3035;
WITH cte_3036 AS (SELECT 3036 AS n) SELECT n FROM cte_3036;
UPDATE bench_t_29 SET payload = 3037 WHERE id = 29;
DELETE FROM bench_t_30 WHERE id = 14;
/* block header 3039 */
DELETE FROM bench_t_0 WHERE id = 0;
-- line 3041: deterministic comment
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
/* block header 3043 */
BEGIN; SELECT 3044; COMMIT;
# hash comment 3045
# hash comment 3046
-- line 3047: deterministic comment
# hash comment 3048
# hash comment 3049
UPDATE bench_t_42 SET payload = 3050 WHERE id = 10;
-- line 3051: deterministic comment
BEGIN; SELECT 3052; COMMIT;
$dz$ dollar body 3053 ; semicolon inside $dz$
UPDATE bench_t_46 SET payload = 3054 WHERE id = 14;
WITH cte_3055 AS (SELECT 3055 AS n) SELECT n FROM cte_3055;
DELETE FROM bench_t_16 WHERE id = 0;
WITH cte_3057 AS (SELECT 3057 AS n) SELECT n FROM cte_3057;
SELECT * FROM "quoted_3058" WHERE col = E'esc\'3058';
SELECT `mysql_3059` FROM `tbl_9`;
# hash comment 3060
BEGIN; SELECT 3061; COMMIT;
# hash comment 3062
DELETE FROM bench_t_23 WHERE id = 7;
WITH cte_3064 AS (SELECT 3064 AS n) SELECT n FROM cte_3064;
BEGIN; SELECT 3065; COMMIT;
# hash comment 3066
WITH cte_3067 AS (SELECT 3067 AS n) SELECT n FROM cte_3067;
UPDATE bench_t_60 SET payload = 3068 WHERE id = 28;
-- line 3069: deterministic comment
SELECT * FROM "quoted_3070" WHERE col = E'esc\'3070';
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
INSERT INTO bench_t_0 (id, payload) VALUES (3072, 'v3072');
WITH cte_3073 AS (SELECT 3073 AS n) SELECT n FROM cte_3073;
INSERT INTO bench_t_2 (id, payload) VALUES (3074, 'v3074');
-- line 3075: deterministic comment
/* block header 3076 */
WITH cte_3077 AS (SELECT 3077 AS n) SELECT n FROM cte_3077;
SELECT * FROM "quoted_3078" WHERE col = E'esc\'3078';
INSERT INTO bench_t_7 (id, payload) VALUES (3079, 'v3079');
-- line 3080: deterministic comment
/* block header 3081 */
$dz$ dollar body 3082 ; semicolon inside $dz$
# hash comment 3083
SELECT 3084 AS id, 'row_3084' AS label;
SELECT [bracket_3085] FROM [dbo].[tbl_5];
$dz$ dollar body 3086 ; semicolon inside $dz$
SELECT * FROM "quoted_3087" WHERE col = E'esc\'3087';
WITH cte_3088 AS (SELECT 3088 AS n) SELECT n FROM cte_3088;
DELETE FROM bench_t_17 WHERE id = 1;
BEGIN; SELECT 3090; COMMIT;
DELETE FROM bench_t_19 WHERE id = 3;
SELECT `mysql_3092` FROM `tbl_42`;
SELECT `mysql_3093` FROM `tbl_43`;
BEGIN; SELECT 3094; COMMIT;
SELECT 3095 AS id, 'row_3095' AS label;
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
/* block header 3097 */
SELECT 3098 AS id, 'row_3098' AS label;
BEGIN; SELECT 3099; COMMIT;
SELECT * FROM "quoted_3100" WHERE col = E'esc\'3100';
SELECT [bracket_3101] FROM [dbo].[tbl_21];
WITH cte_3102 AS (SELECT 3102 AS n) SELECT n FROM cte_3102;
UPDATE bench_t_31 SET payload = 3103 WHERE id = 31;
SELECT * FROM "quoted_3104" WHERE col = E'esc\'3104';
DELETE FROM bench_t_1 WHERE id = 1;
WITH cte_3106 AS (SELECT 3106 AS n) SELECT n FROM cte_3106;
# hash comment 3107
INSERT INTO bench_t_36 (id, payload) VALUES (3108, 'v3108');
SELECT 3109 AS id, 'row_3109' AS label;
INSERT INTO bench_t_38 (id, payload) VALUES (3110, 'v3110');
WITH cte_3111 AS (SELECT 3111 AS n) SELECT n FROM cte_3111;
UPDATE bench_t_40 SET payload = 3112 WHERE id = 8;
SELECT `mysql_3113` FROM `tbl_13`;
SELECT `mysql_3114` FROM `tbl_14`;
SELECT `mysql_3115` FROM `tbl_15`;
-- line 3116: deterministic comment
SELECT nested FROM t WHERE id IN (3117, 3118, 3119);
INSERT INTO bench_t_46 (id, payload) VALUES (3118, 'v3118');
$dz$ dollar body 3119 ; semicolon inside $dz$
INSERT INTO bench_t_48 (id, payload) VALUES (3120, 'v3120');
SELECT nested FROM t WHERE id IN (3121, 3122, 3123);
-- line 3122: deterministic comment
SELECT [bracket_3123] FROM [dbo].[tbl_3];
DELETE FROM bench_t_20 WHERE id = 4;
WITH cte_3125 AS (SELECT 3125 AS n) SELECT n FROM cte_3125;
# hash comment 3126
BEGIN; SELECT 3127; COMMIT;
# hash comment 3128
WITH cte_3129 AS (SELECT 3129 AS n) SELECT n FROM cte_3129;
DELETE FROM bench_t_26 WHERE id = 10;
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT [bracket_3132] FROM [dbo].[tbl_12];
DELETE FROM bench_t_29 WHERE id = 13;
# hash comment 3134
WITH cte_3135 AS (SELECT 3135 AS n) SELECT n FROM cte_3135;
-- line 3136: deterministic comment
DELETE FROM bench_t_1 WHERE id = 1;
# hash comment 3138
SELECT 3139 AS id, 'row_3139' AS label;
WITH cte_3140 AS (SELECT 3140 AS n) SELECT n FROM cte_3140;
$dz$ dollar body 3141 ; semicolon inside $dz$
BEGIN; SELECT 3142; COMMIT;
$dz$ dollar body 3143 ; semicolon inside $dz$
INSERT INTO bench_t_72 (id, payload) VALUES (3144, 'v3144');
# hash comment 3145
SELECT `mysql_3146` FROM `tbl_46`;
WITH cte_3147 AS (SELECT 3147 AS n) SELECT n FROM cte_3147;
DELETE FROM bench_t_12 WHERE id = 12;
SELECT 3149 AS id, 'row_3149' AS label;
-- line 3150: deterministic comment
INSERT INTO bench_t_79 (id, payload) VALUES (3151, 'v3151');
-- line 3152: deterministic comment
DELETE FROM bench_t_17 WHERE id = 1;
$dz$ dollar body 3154 ; semicolon inside $dz$
SELECT `mysql_3155` FROM `tbl_5`;
SELECT 3156 AS id, 'row_3156' AS label;
# hash comment 3157
WITH cte_3158 AS (SELECT 3158 AS n) SELECT n FROM cte_3158;
DELETE FROM bench_t_23 WHERE id = 7;
SELECT nested FROM t WHERE id IN (3160, 3161, 3162);
SELECT `mysql_3161` FROM `tbl_11`;
BEGIN; SELECT 3162; COMMIT;
SELECT nested FROM t WHERE id IN (3163, 3164, 3165);
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
$dz$ dollar body 3165 ; semicolon inside $dz$
SELECT [bracket_3166] FROM [dbo].[tbl_6];
$dz$ dollar body 3167 ; semicolon inside $dz$
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT 3169 AS id, 'row_3169' AS label;
DELETE FROM bench_t_2 WHERE id = 2;
WITH cte_3171 AS (SELECT 3171 AS n) SELECT n FROM cte_3171;
/* block header 3172 */
INSERT INTO bench_t_101 (id, payload) VALUES (3173, 'v3173');
BEGIN; SELECT 3174; COMMIT;
DELETE FROM bench_t_7 WHERE id = 7;
SELECT `mysql_3176` FROM `tbl_26`;
SELECT `mysql_3177` FROM `tbl_27`;
INSERT INTO bench_t_106 (id, payload) VALUES (3178, 'v3178');
# hash comment 3179
SELECT 3180 AS id, 'row_3180' AS label;
SELECT * FROM "quoted_3181" WHERE col = E'esc\'3181';
/* block header 3182 */
/* block header 3183 */
-- line 3184: deterministic comment
SELECT nested FROM t WHERE id IN (3185, 3186, 3187);
-- line 3186: deterministic comment
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT [bracket_3188] FROM [dbo].[tbl_28];
-- line 3189: deterministic comment
SELECT [bracket_3190] FROM [dbo].[tbl_30];
UPDATE bench_t_55 SET payload = 3191 WHERE id = 23;
SELECT 3192 AS id, 'row_3192' AS label;
# hash comment 3193
INSERT INTO bench_t_122 (id, payload) VALUES (3194, 'v3194');
$dz$ dollar body 3195 ; semicolon inside $dz$
WITH cte_3196 AS (SELECT 3196 AS n) SELECT n FROM cte_3196;
/* block header 3197 */
# hash comment 3198
SELECT [bracket_3199] FROM [dbo].[tbl_39];
/* block header 3200 */
SELECT nested FROM t WHERE id IN (3201, 3202, 3203);
# hash comment 3202
DELETE FROM bench_t_3 WHERE id = 3;
SELECT 3204 AS id, 'row_3204' AS label;
-- line 3205: deterministic comment
INSERT INTO bench_t_6 (id, payload) VALUES (3206, 'v3206');
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
/* block header 3208 */
DELETE FROM bench_t_9 WHERE id = 9;
INSERT INTO bench_t_10 (id, payload) VALUES (3210, 'v3210');
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
BEGIN; SELECT 3212; COMMIT;
BEGIN; SELECT 3213; COMMIT;
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
-- line 3215: deterministic comment
SELECT nested FROM t WHERE id IN (3216, 3217, 3218);
DELETE FROM bench_t_17 WHERE id = 1;
SELECT 3218 AS id, 'row_3218' AS label;
BEGIN; SELECT 3219; COMMIT;
SELECT [bracket_3220] FROM [dbo].[tbl_20];
SELECT nested FROM t WHERE id IN (3221, 3222, 3223);
SELECT nested FROM t WHERE id IN (3222, 3223, 3224);
# hash comment 3223
SELECT nested FROM t WHERE id IN (3224, 3225, 3226);
$dz$ dollar body 3225 ; semicolon inside $dz$
INSERT INTO bench_t_26 (id, payload) VALUES (3226, 'v3226');
SELECT nested FROM t WHERE id IN (3227, 3228, 3229);
SELECT * FROM "quoted_3228" WHERE col = E'esc\'3228';
WITH cte_3229 AS (SELECT 3229 AS n) SELECT n FROM cte_3229;
WITH cte_3230 AS (SELECT 3230 AS n) SELECT n FROM cte_3230;
SELECT nested FROM t WHERE id IN (3231, 3232, 3233);
SELECT [bracket_3232] FROM [dbo].[tbl_32];
BEGIN; SELECT 3233; COMMIT;
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
-- line 3235: deterministic comment
-- line 3236: deterministic comment
DELETE FROM bench_t_5 WHERE id = 5;
SELECT * FROM "quoted_3238" WHERE col = E'esc\'3238';
BEGIN; SELECT 3239; COMMIT;
$dz$ dollar body 3240 ; semicolon inside $dz$
WITH cte_3241 AS (SELECT 3241 AS n) SELECT n FROM cte_3241;
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
BEGIN; SELECT 3243; COMMIT;
BEGIN; SELECT 3244; COMMIT;
DELETE FROM bench_t_13 WHERE id = 13;
DELETE FROM bench_t_14 WHERE id = 14;
SELECT * FROM "quoted_3247" WHERE col = E'esc\'3247';
SELECT 3248 AS id, 'row_3248' AS label;
SELECT nested FROM t WHERE id IN (3249, 3250, 3251);
/*
 * section 13
 * checksum 8a07
 */
SELECT nested FROM t WHERE id IN (3250, 3251, 3252);
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
BEGIN; SELECT 3256; COMMIT;
-- line 3257: deterministic comment
SELECT [bracket_3258] FROM [dbo].[tbl_18];
SELECT nested FROM t WHERE id IN (3259, 3260, 3261);
SELECT 3260 AS id, 'row_3260' AS label;
/* block header 3261 */
UPDATE bench_t_62 SET payload = 3262 WHERE id = 30;
SELECT [bracket_3263] FROM [dbo].[tbl_23];
DELETE FROM bench_t_0 WHERE id = 0;
UPDATE bench_t_1 SET payload = 3265 WHERE id = 1;
WITH cte_3266 AS (SELECT 3266 AS n) SELECT n FROM cte_3266;
SELECT 3267 AS id, 'row_3267' AS label;
SELECT [bracket_3268] FROM [dbo].[tbl_28];
UPDATE bench_t_5 SET payload = 3269 WHERE id = 5;
$dz$ dollar body 3270 ; semicolon inside $dz$
SELECT `mysql_3271` FROM `tbl_21`;
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
# hash comment 3273
UPDATE bench_t_10 SET payload = 3274 WHERE id = 10;
SELECT * FROM "quoted_3275" WHERE col = E'esc\'3275';
DELETE FROM bench_t_12 WHERE id = 12;
INSERT INTO bench_t_77 (id, payload) VALUES (3277, 'v3277');
SELECT * FROM "quoted_3278" WHERE col = E'esc\'3278';
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
BEGIN; SELECT 3280; COMMIT;
UPDATE bench_t_17 SET payload = 3281 WHERE id = 17;
SELECT [bracket_3282] FROM [dbo].[tbl_2];
# hash comment 3283
DELETE FROM bench_t_20 WHERE id = 4;
-- line 3285: deterministic comment
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
/* block header 3287 */
INSERT INTO bench_t_88 (id, payload) VALUES (3288, 'v3288');
WITH cte_3289 AS (SELECT 3289 AS n) SELECT n FROM cte_3289;
UPDATE bench_t_26 SET payload = 3290 WHERE id = 26;
SELECT 3291 AS id, 'row_3291' AS label;
-- line 3292: deterministic comment
SELECT nested FROM t WHERE id IN (3293, 3294, 3295);
SELECT nested FROM t WHERE id IN (3294, 3295, 3296);
UPDATE bench_t_31 SET payload = 3295 WHERE id = 31;
UPDATE bench_t_32 SET payload = 3296 WHERE id = 0;
BEGIN; SELECT 3297; COMMIT;
-- line 3298: deterministic comment
SELECT `mysql_3299` FROM `tbl_49`;
WITH cte_3300 AS (SELECT 3300 AS n) SELECT n FROM cte_3300;
-- line 3301: deterministic comment
UPDATE bench_t_38 SET payload = 3302 WHERE id = 6;
/* block header 3303 */
BEGIN; SELECT 3304; COMMIT;
SELECT * FROM "quoted_3305" WHERE col = E'esc\'3305';
SELECT [bracket_3306] FROM [dbo].[tbl_26];
DELETE FROM bench_t_11 WHERE id = 11;
SELECT * FROM "quoted_3308" WHERE col = E'esc\'3308';
WITH cte_3309 AS (SELECT 3309 AS n) SELECT n FROM cte_3309;
SELECT nested FROM t WHERE id IN (3310, 3311, 3312);
SELECT `mysql_3311` FROM `tbl_11`;
BEGIN; SELECT 3312; COMMIT;
UPDATE bench_t_49 SET payload = 3313 WHERE id = 17;
WITH cte_3314 AS (SELECT 3314 AS n) SELECT n FROM cte_3314;
SELECT 3315 AS id, 'row_3315' AS label;
$dz$ dollar body 3316 ; semicolon inside $dz$
SELECT `mysql_3317` FROM `tbl_17`;
-- line 3318: deterministic comment
SELECT `mysql_3319` FROM `tbl_19`;
WITH cte_3320 AS (SELECT 3320 AS n) SELECT n FROM cte_3320;
SELECT [bracket_3321] FROM [dbo].[tbl_1];
INSERT INTO bench_t_122 (id, payload) VALUES (3322, 'O''Brien');
# hash comment 3323
SELECT [bracket_3324] FROM [dbo].[tbl_4];
UPDATE bench_t_61 SET payload = 3325 WHERE id = 29;
DELETE FROM bench_t_30 WHERE id = 14;
INSERT INTO bench_t_127 (id, payload) VALUES (3327, 'v3327');
SELECT 3328 AS id, 'row_3328' AS label;
INSERT INTO bench_t_1 (id, payload) VALUES (3329, 'v3329');
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT `mysql_3331` FROM `tbl_31`;
WITH cte_3332 AS (SELECT 3332 AS n) SELECT n FROM cte_3332;
WITH cte_3333 AS (SELECT 3333 AS n) SELECT n FROM cte_3333;
-- line 3334: deterministic comment
$dz$ dollar body 3335 ; semicolon inside $dz$
SELECT nested FROM t WHERE id IN (3336, 3337, 3338);
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
# hash comment 3338
$dz$ dollar body 3339 ; semicolon inside $dz$
$dz$ dollar body 3340 ; semicolon inside $dz$
/* block header 3341 */
SELECT `mysql_3342` FROM `tbl_42`;
SELECT nested FROM t WHERE id IN (3343, 3344, 3345);
-- line 3344: deterministic comment
/* block header 3345 */
WITH cte_3346 AS (SELECT 3346 AS n) SELECT n FROM cte_3346;
SELECT [bracket_3347] FROM [dbo].[tbl_27];
$dz$ dollar body 3348 ; semicolon inside $dz$
SELECT `mysql_3349` FROM `tbl_49`;
SELECT * FROM "quoted_3350" WHERE col = E'esc\'3350';
$dz$ dollar body 3351 ; semicolon inside $dz$
WITH cte_3352 AS (SELECT 3352 AS n) SELECT n FROM cte_3352;
$dz$ dollar body 3353 ; semicolon inside $dz$
# hash comment 3354
SELECT * FROM "quoted_3355" WHERE col = E'esc\'3355';
DELETE FROM bench_t_28 WHERE id = 12;
-- line 3357: deterministic comment
SELECT nested FROM t WHERE id IN (3358, 3359, 3360);
SELECT [bracket_3359] FROM [dbo].[tbl_39];
/* block header 3360 */
WITH cte_3361 AS (SELECT 3361 AS n) SELECT n FROM cte_3361;
SELECT * FROM "quoted_3362" WHERE col = E'esc\'3362';
DELETE FROM bench_t_3 WHERE id = 3;
SELECT * FROM "quoted_3364" WHERE col = E'esc\'3364';
$dz$ dollar body 3365 ; semicolon inside $dz$
SELECT 3366 AS id, 'row_3366' AS label;
SELECT * FROM "quoted_3367" WHERE col = E'esc\'3367';
$dz$ dollar body 3368 ; semicolon inside $dz$
BEGIN; SELECT 3369; COMMIT;
$dz$ dollar body 3370 ; semicolon inside $dz$
SELECT [bracket_3371] FROM [dbo].[tbl_11];
/* block header 3372 */
$dz$ dollar body 3373 ; semicolon inside $dz$
-- line 3374: deterministic comment
-- line 3375: deterministic comment
WITH cte_3376 AS (SELECT 3376 AS n) SELECT n FROM cte_3376;
# hash comment 3377
SELECT `mysql_3378` FROM `tbl_28`;
# hash comment 3379
-- line 3380: deterministic comment
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT nested FROM t WHERE id IN (3382, 3383, 3384);
SELECT `mysql_3383` FROM `tbl_33`;
$dz$ dollar body 3384 ; semicolon inside $dz$
WITH cte_3385 AS (SELECT 3385 AS n) SELECT n FROM cte_3385;
-- line 3386: deterministic comment
# hash comment 3387
SELECT `mysql_3388` FROM `tbl_38`;
SELECT nested FROM t WHERE id IN (3389, 3390, 3391);
SELECT * FROM "quoted_3390" WHERE col = E'esc\'3390';
BEGIN; SELECT 3391; COMMIT;
$dz$ dollar body 3392 ; semicolon inside $dz$
-- line 3393: deterministic comment
SELECT [bracket_3394] FROM [dbo].[tbl_34];
UPDATE bench_t_3 SET payload = 3395 WHERE id = 3;
-- line 3396: deterministic comment
SELECT [bracket_3397] FROM [dbo].[tbl_37];
# hash comment 3398
INSERT INTO bench_t_71 (id, payload) VALUES (3399, 'O''Brien');
$dz$ dollar body 3400 ; semicolon inside $dz$
-- line 3401: deterministic comment
SELECT [bracket_3402] FROM [dbo].[tbl_2];
# hash comment 3403
SELECT * FROM "quoted_3404" WHERE col = E'esc\'3404';
SELECT nested FROM t WHERE id IN (3405, 3406, 3407);
BEGIN; SELECT 3406; COMMIT;
WITH cte_3407 AS (SELECT 3407 AS n) SELECT n FROM cte_3407;
INSERT INTO bench_t_80 (id, payload) VALUES (3408, 'v3408');
DELETE FROM bench_t_17 WHERE id = 1;
WITH cte_3410 AS (SELECT 3410 AS n) SELECT n FROM cte_3410;
/* block header 3411 */
$dz$ dollar body 3412 ; semicolon inside $dz$
SELECT `mysql_3413` FROM `tbl_13`;
SELECT [bracket_3414] FROM [dbo].[tbl_14];
SELECT nested FROM t WHERE id IN (3415, 3416, 3417);
DELETE FROM bench_t_24 WHERE id = 8;
INSERT INTO bench_t_89 (id, payload) VALUES (3417, 'v3417');
SELECT nested FROM t WHERE id IN (3418, 3419, 3420);
SELECT nested FROM t WHERE id IN (3419, 3420, 3421);
SELECT `mysql_3420` FROM `tbl_20`;
INSERT INTO bench_t_93 (id, payload) VALUES (3421, 'O''Brien');
INSERT INTO bench_t_94 (id, payload) VALUES (3422, 'v3422');
$dz$ dollar body 3423 ; semicolon inside $dz$
UPDATE bench_t_32 SET payload = 3424 WHERE id = 0;
INSERT INTO bench_t_97 (id, payload) VALUES (3425, 'v3425');
SELECT `mysql_3426` FROM `tbl_26`;
/* block header 3427 */
SELECT * FROM "quoted_3428" WHERE col = E'esc\'3428';
SELECT * FROM "quoted_3429" WHERE col = E'esc\'3429';
SELECT `mysql_3430` FROM `tbl_30`;
WITH cte_3431 AS (SELECT 3431 AS n) SELECT n FROM cte_3431;
UPDATE bench_t_40 SET payload = 3432 WHERE id = 8;
UPDATE bench_t_41 SET payload = 3433 WHERE id = 9;
# hash comment 3434
UPDATE bench_t_43 SET payload = 3435 WHERE id = 11;
SELECT [bracket_3436] FROM [dbo].[tbl_36];
$dz$ dollar body 3437 ; semicolon inside $dz$
-- line 3438: deterministic comment
-- line 3439: deterministic comment
SELECT `mysql_3440` FROM `tbl_40`;
BEGIN; SELECT 3441; COMMIT;
WITH cte_3442 AS (SELECT 3442 AS n) SELECT n FROM cte_3442;
SELECT `mysql_3443` FROM `tbl_43`;
/* block header 3444 */
SELECT 3445 AS id, 'row_3445' AS label;
SELECT `mysql_3446` FROM `tbl_46`;
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
$dz$ dollar body 3448 ; semicolon inside $dz$
UPDATE bench_t_57 SET payload = 3449 WHERE id = 25;
BEGIN; SELECT 3450; COMMIT;
-- line 3451: deterministic comment
SELECT * FROM "quoted_3452" WHERE col = E'esc\'3452';
SELECT [bracket_3453] FROM [dbo].[tbl_13];
SELECT * FROM "quoted_3454" WHERE col = E'esc\'3454';
INSERT INTO bench_t_127 (id, payload) VALUES (3455, 'v3455');
$dz$ dollar body 3456 ; semicolon inside $dz$
UPDATE bench_t_1 SET payload = 3457 WHERE id = 1;
UPDATE bench_t_2 SET payload = 3458 WHERE id = 2;
WITH cte_3459 AS (SELECT 3459 AS n) SELECT n FROM cte_3459;
SELECT [bracket_3460] FROM [dbo].[tbl_20];
WITH cte_3461 AS (SELECT 3461 AS n) SELECT n FROM cte_3461;
$dz$ dollar body 3462 ; semicolon inside $dz$
SELECT * FROM "quoted_3463" WHERE col = E'esc\'3463';
SELECT `mysql_3464` FROM `tbl_14`;
BEGIN; SELECT 3465; COMMIT;
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
/* block header 3467 */
BEGIN; SELECT 3468; COMMIT;
SELECT [bracket_3469] FROM [dbo].[tbl_29];
BEGIN; SELECT 3470; COMMIT;
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT 3472 AS id, 'row_3472' AS label;
BEGIN; SELECT 3473; COMMIT;
SELECT nested FROM t WHERE id IN (3474, 3475, 3476);
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
/* block header 3476 */
SELECT `mysql_3477` FROM `tbl_27`;
-- line 3478: deterministic comment
-- line 3479: deterministic comment
SELECT [bracket_3480] FROM [dbo].[tbl_0];
SELECT [bracket_3481] FROM [dbo].[tbl_1];
-- line 3482: deterministic comment
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
WITH cte_3484 AS (SELECT 3484 AS n) SELECT n FROM cte_3484;
SELECT 3485 AS id, 'row_3485' AS label;
SELECT * FROM "quoted_3486" WHERE col = E'esc\'3486';
/* block header 3487 */
BEGIN; SELECT 3488; COMMIT;
DELETE FROM bench_t_1 WHERE id = 1;
-- line 3490: deterministic comment
UPDATE bench_t_35 SET payload = 3491 WHERE id = 3;
UPDATE bench_t_36 SET payload = 3492 WHERE id = 4;
UPDATE bench_t_37 SET payload = 3493 WHERE id = 5;
WITH cte_3494 AS (SELECT 3494 AS n) SELECT n FROM cte_3494;
SELECT 3495 AS id, 'row_3495' AS label;
/* block header 3496 */
SELECT nested FROM t WHERE id IN (3497, 3498, 3499);
SELECT 3498 AS id, 'row_3498' AS label;
SELECT nested FROM t WHERE id IN (3499, 3500, 3501);
/*
 * section 14
 * checksum e33a
 */
WITH cte_3500 AS (SELECT 3500 AS n) SELECT n FROM cte_3500;
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
/* block header 3506 */
SELECT * FROM "quoted_3507" WHERE col = E'esc\'3507';
SELECT nested FROM t WHERE id IN (3508, 3509, 3510);
DELETE FROM bench_t_21 WHERE id = 5;
SELECT `mysql_3510` FROM `tbl_10`;
SELECT * FROM "quoted_3511" WHERE col = E'esc\'3511';
-- line 3512: deterministic comment
DELETE FROM bench_t_25 WHERE id = 9;
SELECT [bracket_3514] FROM [dbo].[tbl_34];
SELECT `mysql_3515` FROM `tbl_15`;
DELETE FROM bench_t_28 WHERE id = 12;
INSERT INTO bench_t_61 (id, payload) VALUES (3517, 'v3517');
SELECT nested FROM t WHERE id IN (3518, 3519, 3520);
INSERT INTO bench_t_63 (id, payload) VALUES (3519, 'v3519');
SELECT 3520 AS id, 'row_3520' AS label;
SELECT `mysql_3521` FROM `tbl_21`;
-- line 3522: deterministic comment
-- line 3523: deterministic comment
# hash comment 3524
DELETE FROM bench_t_5 WHERE id = 5;
/* block header 3526 */
SELECT * FROM "quoted_3527" WHERE col = E'esc\'3527';
UPDATE bench_t_8 SET payload = 3528 WHERE id = 8;
UPDATE bench_t_9 SET payload = 3529 WHERE id = 9;
SELECT `mysql_3530` FROM `tbl_30`;
DELETE FROM bench_t_11 WHERE id = 11;
DELETE FROM bench_t_12 WHERE id = 12;
# hash comment 3533
/* block header 3534 */
BEGIN; SELECT 3535; COMMIT;
SELECT `mysql_3536` FROM `tbl_36`;
SELECT 3537 AS id, 'row_3537' AS label;
-- line 3538: deterministic comment
DELETE FROM bench_t_19 WHERE id = 3;
SELECT * FROM "quoted_3540" WHERE col = E'esc\'3540';
SELECT 3541 AS id, 'row_3541' AS label;
BEGIN; SELECT 3542; COMMIT;
UPDATE bench_t_23 SET payload = 3543 WHERE id = 23;
INSERT INTO bench_t_88 (id, payload) VALUES (3544, 'v3544');
BEGIN; SELECT 3545; COMMIT;
SELECT * FROM "quoted_3546" WHERE col = E'esc\'3546';
SELECT nested FROM t WHERE id IN (3547, 3548, 3549);
WITH cte_3548 AS (SELECT 3548 AS n) SELECT n FROM cte_3548;
-- line 3549: deterministic comment
SELECT `mysql_3550` FROM `tbl_0`;
DELETE FROM bench_t_31 WHERE id = 15;
SELECT [bracket_3552] FROM [dbo].[tbl_32];
UPDATE bench_t_33 SET payload = 3553 WHERE id = 1;
SELECT 3554 AS id, 'row_3554' AS label;
SELECT [bracket_3555] FROM [dbo].[tbl_35];
-- line 3556: deterministic comment
BEGIN; SELECT 3557; COMMIT;
UPDATE bench_t_38 SET payload = 3558 WHERE id = 6;
# hash comment 3559
SELECT `mysql_3560` FROM `tbl_10`;
SELECT 3561 AS id, 'row_3561' AS label;
INSERT INTO bench_t_106 (id, payload) VALUES (3562, 'v3562');
SELECT `mysql_3563` FROM `tbl_13`;
SELECT * FROM "quoted_3564" WHERE col = E'esc\'3564';
SELECT `mysql_3565` FROM `tbl_15`;
BEGIN; SELECT 3566; COMMIT;
WITH cte_3567 AS (SELECT 3567 AS n) SELECT n FROM cte_3567;
INSERT INTO bench_t_112 (id, payload) VALUES (3568, 'v3568');
UPDATE bench_t_49 SET payload = 3569 WHERE id = 17;
SELECT [bracket_3570] FROM [dbo].[tbl_10];
WITH cte_3571 AS (SELECT 3571 AS n) SELECT n FROM cte_3571;
UPDATE bench_t_52 SET payload = 3572 WHERE id = 20;
# hash comment 3573
DELETE FROM bench_t_22 WHERE id = 6;
SELECT * FROM "quoted_3575" WHERE col = E'esc\'3575';
INSERT INTO bench_t_120 (id, payload) VALUES (3576, 'v3576');
INSERT INTO bench_t_121 (id, payload) VALUES (3577, 'v3577');
SELECT * FROM "quoted_3578" WHERE col = E'esc\'3578';
SELECT [bracket_3579] FROM [dbo].[tbl_19];
INSERT INTO bench_t_124 (id, payload) VALUES (3580, 'v3580');
UPDATE bench_t_61 SET payload = 3581 WHERE id = 29;
-- line 3582: deterministic comment
SELECT nested FROM t WHERE id IN (3583, 3584, 3585);
SELECT [bracket_3584] FROM [dbo].[tbl_24];
SELECT 3585 AS id, 'row_3585' AS label;
INSERT INTO bench_t_2 (id, payload) VALUES (3586, 'O''Brien');
SELECT nested FROM t WHERE id IN (3587, 3588, 3589);
WITH cte_3588 AS (SELECT 3588 AS n) SELECT n FROM cte_3588;
UPDATE bench_t_5 SET payload = 3589 WHERE id = 5;
DELETE FROM bench_t_6 WHERE id = 6;
/* block header 3591 */
WITH cte_3592 AS (SELECT 3592 AS n) SELECT n FROM cte_3592;
SELECT nested FROM t WHERE id IN (3593, 3594, 3595);
SELECT 3594 AS id, 'row_3594' AS label;
/* block header 3595 */
SELECT nested FROM t WHERE id IN (3596, 3597, 3598);
WITH cte_3597 AS (SELECT 3597 AS n) SELECT n FROM cte_3597;
SELECT [bracket_3598] FROM [dbo].[tbl_38];
/* block header 3599 */
SELECT * FROM "quoted_3600" WHERE col = E'esc\'3600';
SELECT 3601 AS id, 'row_3601' AS label;
SELECT [bracket_3602] FROM [dbo].[tbl_2];
# hash comment 3603
DELETE FROM bench_t_20 WHERE id = 4;
/* block header 3605 */
SELECT `mysql_3606` FROM `tbl_6`;
INSERT INTO bench_t_23 (id, payload) VALUES (3607, 'v3607');
SELECT `mysql_3608` FROM `tbl_8`;
# hash comment 3609
WITH cte_3610 AS (SELECT 3610 AS n) SELECT n FROM cte_3610;
UPDATE bench_t_27 SET payload = 3611 WHERE id = 27;
/* block header 3612 */
-- line 3613: deterministic comment
BEGIN; SELECT 3614; COMMIT;
SELECT 3615 AS id, 'row_3615' AS label;
SELECT nested FROM t WHERE id IN (3616, 3617, 3618);
SELECT nested FROM t WHERE id IN (3617, 3618, 3619);
$dz$ dollar body 3618 ; semicolon inside $dz$
-- line 3619: deterministic comment
SELECT `mysql_3620` FROM `tbl_20`;
$dz$ dollar body 3621 ; semicolon inside $dz$
SELECT nested FROM t WHERE id IN (3622, 3623, 3624);
SELECT 3623 AS id, 'row_3623' AS label;
SELECT * FROM "quoted_3624" WHERE col = E'esc\'3624';
SELECT * FROM "quoted_3625" WHERE col = E'esc\'3625';
WITH cte_3626 AS (SELECT 3626 AS n) SELECT n FROM cte_3626;
SELECT 3627 AS id, 'row_3627' AS label;
-- line 3628: deterministic comment
SELECT [bracket_3629] FROM [dbo].[tbl_29];
DELETE FROM bench_t_14 WHERE id = 14;
/* block header 3631 */
SELECT 3632 AS id, 'row_3632' AS label;
BEGIN; SELECT 3633; COMMIT;
UPDATE bench_t_50 SET payload = 3634 WHERE id = 18;
SELECT nested FROM t WHERE id IN (3635, 3636, 3637);
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
$dz$ dollar body 3637 ; semicolon inside $dz$
BEGIN; SELECT 3638; COMMIT;
/* block header 3639 */
SELECT nested FROM t WHERE id IN (3640, 3641, 3642);
INSERT INTO bench_t_57 (id, payload) VALUES (3641, 'O''Brien');
/* block header 3642 */
INSERT INTO bench_t_59 (id, payload) VALUES (3643, 'v3643');
SELECT * FROM "quoted_3644" WHERE col = E'esc\'3644';
SELECT nested FROM t WHERE id IN (3645, 3646, 3647);
DELETE FROM bench_t_30 WHERE id = 14;
WITH cte_3647 AS (SELECT 3647 AS n) SELECT n FROM cte_3647;
DELETE FROM bench_t_0 WHERE id = 0;
BEGIN; SELECT 3649; COMMIT;
/* block header 3650 */
$dz$ dollar body 3651 ; semicolon inside $dz$
UPDATE bench_t_4 SET payload = 3652 WHERE id = 4;
SELECT 3653 AS id, 'row_3653' AS label;
INSERT INTO bench_t_70 (id, payload) VALUES (3654, 'v3654');
DELETE FROM bench_t_7 WHERE id = 7;
SELECT `mysql_3656` FROM `tbl_6`;
WITH cte_3657 AS (SELECT 3657 AS n) SELECT n FROM cte_3657;
BEGIN; SELECT 3658; COMMIT;
INSERT INTO bench_t_75 (id, payload) VALUES (3659, 'v3659');
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
DELETE FROM bench_t_13 WHERE id = 13;
INSERT INTO bench_t_78 (id, payload) VALUES (3662, 'v3662');
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
/* block header 3664 */
SELECT nested FROM t WHERE id IN (3665, 3666, 3667);
WITH cte_3666 AS (SELECT 3666 AS n) SELECT n FROM cte_3666;
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
WITH cte_3668 AS (SELECT 3668 AS n) SELECT n FROM cte_3668;
DELETE FROM bench_t_21 WHERE id = 5;
UPDATE bench_t_22 SET payload = 3670 WHERE id = 22;
SELECT `mysql_3671` FROM `tbl_21`;
SELECT [bracket_3672] FROM [dbo].[tbl_32];
/* block header 3673 */
# hash comment 3674
WITH cte_3675 AS (SELECT 3675 AS n) SELECT n FROM cte_3675;
SELECT [bracket_3676] FROM [dbo].[tbl_36];
SELECT * FROM "quoted_3677" WHERE col = E'esc\'3677';
INSERT INTO bench_t_94 (id, payload) VALUES (3678, 'v3678');
# hash comment 3679
SELECT [bracket_3680] FROM [dbo].[tbl_0];
SELECT * FROM "quoted_3681" WHERE col = E'esc\'3681';
/* block header 3682 */
/* block header 3683 */
SELECT 3684 AS id, 'row_3684' AS label;
SELECT `mysql_3685` FROM `tbl_35`;
UPDATE bench_t_38 SET payload = 3686 WHERE id = 6;
SELECT `mysql_3687` FROM `tbl_37`;
BEGIN; SELECT 3688; COMMIT;
WITH cte_3689 AS (SELECT 3689 AS n) SELECT n FROM cte_3689;
SELECT `mysql_3690` FROM `tbl_40`;
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
# hash comment 3692
SELECT nested FROM t WHERE id IN (3693, 3694, 3695);
/* block header 3694 */
WITH cte_3695 AS (SELECT 3695 AS n) SELECT n FROM cte_3695;
SELECT [bracket_3696] FROM [dbo].[tbl_16];
SELECT * FROM "quoted_3697" WHERE col = E'esc\'3697';
BEGIN; SELECT 3698; COMMIT;
SELECT nested FROM t WHERE id IN (3699, 3700, 3701);
SELECT * FROM "quoted_3700" WHERE col = E'esc\'3700';
SELECT [bracket_3701] FROM [dbo].[tbl_21];
SELECT [bracket_3702] FROM [dbo].[tbl_22];
$dz$ dollar body 3703 ; semicolon inside $dz$
INSERT INTO bench_t_120 (id, payload) VALUES (3704, 'v3704');
SELECT `mysql_3705` FROM `tbl_5`;
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
INSERT INTO bench_t_123 (id, payload) VALUES (3707, 'O''Brien');
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
BEGIN; SELECT 3709; COMMIT;
-- line 3710: deterministic comment
INSERT INTO bench_t_127 (id, payload) VALUES (3711, 'v3711');
BEGIN; SELECT 3712; COMMIT;
BEGIN; SELECT 3713; COMMIT;
SELECT * FROM "quoted_3714" WHERE col = E'esc\'3714';
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
/* block header 3716 */
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT nested FROM t WHERE id IN (3718, 3719, 3720);
SELECT 3719 AS id, 'row_3719' AS label;
SELECT `mysql_3720` FROM `tbl_20`;
SELECT * FROM "quoted_3721" WHERE col = E'esc\'3721';
INSERT INTO bench_t_10 (id, payload) VALUES (3722, 'v3722');
SELECT 3723 AS id, 'row_3723' AS label;
DELETE FROM bench_t_12 WHERE id = 12;
SELECT 3725 AS id, 'row_3725' AS label;
SELECT `mysql_3726` FROM `tbl_26`;
SELECT 3727 AS id, 'row_3727' AS label;
SELECT nested FROM t WHERE id IN (3728, 3729, 3730);
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
-- line 3730: deterministic comment
SELECT [bracket_3731] FROM [dbo].[tbl_11];
SELECT [bracket_3732] FROM [dbo].[tbl_12];
SELECT 3733 AS id, 'row_3733' AS label;
WITH cte_3734 AS (SELECT 3734 AS n) SELECT n FROM cte_3734;
# hash comment 3735
# hash comment 3736
-- line 3737: deterministic comment
SELECT nested FROM t WHERE id IN (3738, 3739, 3740);
SELECT [bracket_3739] FROM [dbo].[tbl_19];
SELECT nested FROM t WHERE id IN (3740, 3741, 3742);
# hash comment 3741
SELECT * FROM "quoted_3742" WHERE col = E'esc\'3742';
# hash comment 3743
/* block header 3744 */
$dz$ dollar body 3745 ; semicolon inside $dz$
DELETE FROM bench_t_2 WHERE id = 2;
UPDATE bench_t_35 SET payload = 3747 WHERE id = 3;
WITH cte_3748 AS (SELECT 3748 AS n) SELECT n FROM cte_3748;
DELETE FROM bench_t_5 WHERE id = 5;
/*
 * section 15
 * checksum 2f6f
 */
WITH cte_3750 AS (SELECT 3750 AS n) SELECT n FROM cte_3750;
BEGIN; SELECT 3755; COMMIT;
-- line 3756: deterministic comment
# hash comment 3757
WITH cte_3758 AS (SELECT 3758 AS n) SELECT n FROM cte_3758;
DELETE FROM bench_t_15 WHERE id = 15;
DELETE FROM bench_t_16 WHERE id = 0;
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT * FROM "quoted_3762" WHERE col = E'esc\'3762';
SELECT nested FROM t WHERE id IN (3763, 3764, 3765);
SELECT [bracket_3764] FROM [dbo].[tbl_4];
INSERT INTO bench_t_53 (id, payload) VALUES (3765, 'v3765');
WITH cte_3766 AS (SELECT 3766 AS n) SELECT n FROM cte_3766;
SELECT [bracket_3767] FROM [dbo].[tbl_7];
WITH cte_3768 AS (SELECT 3768 AS n) SELECT n FROM cte_3768;
SELECT * FROM "quoted_3769" WHERE col = E'esc\'3769';
UPDATE bench_t_58 SET payload = 3770 WHERE id = 26;
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
$dz$ dollar body 3772 ; semicolon inside $dz$
SELECT * FROM "quoted_3773" WHERE col = E'esc\'3773';
SELECT [bracket_3774] FROM [dbo].[tbl_14];
DELETE FROM bench_t_31 WHERE id = 15;
# hash comment 3776
SELECT * FROM "quoted_3777" WHERE col = E'esc\'3777';
INSERT INTO bench_t_66 (id, payload) VALUES (3778, 'v3778');
UPDATE bench_t_3 SET payload = 3779 WHERE id = 3;
INSERT INTO bench_t_68 (id, payload) VALUES (3780, 'v3780');
# hash comment 3781
UPDATE bench_t_6 SET payload = 3782 WHERE id = 6;
/* block header 3783 */
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
BEGIN; SELECT 3785; COMMIT;
SELECT * FROM "quoted_3786" WHERE col = E'esc\'3786';
# hash comment 3787
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
INSERT INTO bench_t_77 (id, payload) VALUES (3789, 'v3789');
SELECT `mysql_3790` FROM `tbl_40`;
UPDATE bench_t_15 SET payload = 3791 WHERE id = 15;
SELECT * FROM "quoted_3792" WHERE col = E'esc\'3792';
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
WITH cte_3794 AS (SELECT 3794 AS n) SELECT n FROM cte_3794;
BEGIN; SELECT 3795; COMMIT;
/* block header 3796 */
SELECT `mysql_3797` FROM `tbl_47`;
SELECT nested FROM t WHERE id IN (3798, 3799, 3800);
DELETE FROM bench_t_23 WHERE id = 7;
-- line 3800: deterministic comment
/* block header 3801 */
UPDATE bench_t_26 SET payload = 3802 WHERE id = 26;
SELECT nested FROM t WHERE id IN (3803, 3804, 3805);
SELECT * FROM "quoted_3804" WHERE col = E'esc\'3804';
DELETE FROM bench_t_29 WHERE id = 13;
DELETE FROM bench_t_30 WHERE id = 14;
UPDATE bench_t_31 SET payload = 3807 WHERE id = 31;
BEGIN; SELECT 3808; COMMIT;
INSERT INTO bench_t_97 (id, payload) VALUES (3809, 'v3809');
SELECT * FROM "quoted_3810" WHERE col = E'esc\'3810';
$dz$ dollar body 3811 ; semicolon inside $dz$
/* block header 3812 */
/* block header 3813 */
BEGIN; SELECT 3814; COMMIT;
SELECT 3815 AS id, 'row_3815' AS label;
INSERT INTO bench_t_104 (id, payload) VALUES (3816, 'v3816');
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT 3818 AS id, 'row_3818' AS label;
WITH cte_3819 AS (SELECT 3819 AS n) SELECT n FROM cte_3819;
SELECT nested FROM t WHERE id IN (3820, 3821, 3822);
/* block header 3821 */
DELETE FROM bench_t_14 WHERE id = 14;
SELECT * FROM "quoted_3823" WHERE col = E'esc\'3823';
INSERT INTO bench_t_112 (id, payload) VALUES (3824, 'v3824');
WITH cte_3825 AS (SELECT 3825 AS n) SELECT n FROM cte_3825;
SELECT `mysql_3826` FROM `tbl_26`;
$dz$ dollar body 3827 ; semicolon inside $dz$
SELECT `mysql_3828` FROM `tbl_28`;
/* block header 3829 */
$dz$ dollar body 3830 ; semicolon inside $dz$
WITH cte_3831 AS (SELECT 3831 AS n) SELECT n FROM cte_3831;
SELECT * FROM "quoted_3832" WHERE col = E'esc\'3832';
/* block header 3833 */
# hash comment 3834
DELETE FROM bench_t_27 WHERE id = 11;
SELECT * FROM "quoted_3836" WHERE col = E'esc\'3836';
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
DELETE FROM bench_t_30 WHERE id = 14;
SELECT * FROM "quoted_3839" WHERE col = E'esc\'3839';
# hash comment 3840
UPDATE bench_t_1 SET payload = 3841 WHERE id = 1;
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
UPDATE bench_t_3 SET payload = 3843 WHERE id = 3;
UPDATE bench_t_4 SET payload = 3844 WHERE id = 4;
DELETE FROM bench_t_5 WHERE id = 5;
UPDATE bench_t_6 SET payload = 3846 WHERE id = 6;
DELETE FROM bench_t_7 WHERE id = 7;
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
BEGIN; SELECT 3849; COMMIT;
SELECT nested FROM t WHERE id IN (3850, 3851, 3852);
# hash comment 3851
SELECT nested FROM t WHERE id IN (3852, 3853, 3854);
SELECT [bracket_3853] FROM [dbo].[tbl_13];
SELECT `mysql_3854` FROM `tbl_4`;
SELECT * FROM "quoted_3855" WHERE col = E'esc\'3855';
SELECT 3856 AS id, 'row_3856' AS label;
# hash comment 3857
SELECT [bracket_3858] FROM [dbo].[tbl_18];
-- line 3859: deterministic comment
SELECT nested FROM t WHERE id IN (3860, 3861, 3862);
/* block header 3861 */
# hash comment 3862
WITH cte_3863 AS (SELECT 3863 AS n) SELECT n FROM cte_3863;
/* block header 3864 */
SELECT * FROM "quoted_3865" WHERE col = E'esc\'3865';
SELECT 3866 AS id, 'row_3866' AS label;
WITH cte_3867 AS (SELECT 3867 AS n) SELECT n FROM cte_3867;
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT 3870 AS id, 'row_3870' AS label;
INSERT INTO bench_t_31 (id, payload) VALUES (3871, 'v3871');
SELECT 3872 AS id, 'row_3872' AS label;
DELETE FROM bench_t_1 WHERE id = 1;
INSERT INTO bench_t_34 (id, payload) VALUES (3874, 'v3874');
UPDATE bench_t_35 SET payload = 3875 WHERE id = 3;
SELECT 3876 AS id, 'row_3876' AS label;
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT nested FROM t WHERE id IN (3878, 3879, 3880);
$dz$ dollar body 3879 ; semicolon inside $dz$
INSERT INTO bench_t_40 (id, payload) VALUES (3880, 'v3880');
/* block header 3881 */
INSERT INTO bench_t_42 (id, payload) VALUES (3882, 'v3882');
$dz$ dollar body 3883 ; semicolon inside $dz$
SELECT 3884 AS id, 'row_3884' AS label;
/* block header 3885 */
INSERT INTO bench_t_46 (id, payload) VALUES (3886, 'v3886');
SELECT nested FROM t WHERE id IN (3887, 3888, 3889);
# hash comment 3888
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
DELETE FROM bench_t_18 WHERE id = 2;
SELECT `mysql_3891` FROM `tbl_41`;
SELECT `mysql_3892` FROM `tbl_42`;
BEGIN; SELECT 3893; COMMIT;
INSERT INTO bench_t_54 (id, payload) VALUES (3894, 'O''Brien');
SELECT [bracket_3895] FROM [dbo].[tbl_15];
UPDATE bench_t_56 SET payload = 3896 WHERE id = 24;
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT 3898 AS id, 'row_3898' AS label;
BEGIN; SELECT 3899; COMMIT;
# hash comment 3900
SELECT * FROM "quoted_3901" WHERE col = E'esc\'3901';
INSERT INTO bench_t_62 (id, payload) VALUES (3902, 'v3902');
$dz$ dollar body 3903 ; semicolon inside $dz$
# hash comment 3904
SELECT `mysql_3905` FROM `tbl_5`;
/* block header 3906 */
SELECT * FROM "quoted_3907" WHERE col = E'esc\'3907';
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
UPDATE bench_t_5 SET payload = 3909 WHERE id = 5;
SELECT 3910 AS id, 'row_3910' AS label;
SELECT 3911 AS id, 'row_3911' AS label;
SELECT `mysql_3912` FROM `tbl_12`;
# hash comment 3913
INSERT INTO bench_t_74 (id, payload) VALUES (3914, 'v3914');
/* block header 3915 */
SELECT 3916 AS id, 'row_3916' AS label;
BEGIN; SELECT 3917; COMMIT;
UPDATE bench_t_14 SET payload = 3918 WHERE id = 14;
SELECT nested FROM t WHERE id IN (3919, 3920, 3921);
INSERT INTO bench_t_80 (id, payload) VALUES (3920, 'v3920');
SELECT [bracket_3921] FROM [dbo].[tbl_1];
WITH cte_3922 AS (SELECT 3922 AS n) SELECT n FROM cte_3922;
BEGIN; SELECT 3923; COMMIT;
SELECT nested FROM t WHERE id IN (3924, 3925, 3926);
SELECT * FROM "quoted_3925" WHERE col = E'esc\'3925';
DELETE FROM bench_t_22 WHERE id = 6;
SELECT [bracket_3927] FROM [dbo].[tbl_7];
$dz$ dollar body 3928 ; semicolon inside $dz$
SELECT [bracket_3929] FROM [dbo].[tbl_9];
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT * FROM "quoted_3931" WHERE col = E'esc\'3931';
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
-- line 3933: deterministic comment
WITH cte_3934 AS (SELECT 3934 AS n) SELECT n FROM cte_3934;
UPDATE bench_t_31 SET payload = 3935 WHERE id = 31;
SELECT * FROM "quoted_3936" WHERE col = E'esc\'3936';
SELECT nested FROM t WHERE id IN (3937, 3938, 3939);
UPDATE bench_t_34 SET payload = 3938 WHERE id = 2;
SELECT `mysql_3939` FROM `tbl_39`;
# hash comment 3940
SELECT `mysql_3941` FROM `tbl_41`;
-- line 3942: deterministic comment
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
-- line 3944: deterministic comment
SELECT 3945 AS id, 'row_3945' AS label;
INSERT INTO bench_t_106 (id, payload) VALUES (3946, 'v3946');
SELECT [bracket_3947] FROM [dbo].[tbl_27];
SELECT `mysql_3948` FROM `tbl_48`;
# hash comment 3949
SELECT * FROM "quoted_3950" WHERE col = E'esc\'3950';
SELECT 3951 AS id, 'row_3951' AS label;
# hash comment 3952
INSERT INTO bench_t_113 (id, payload) VALUES (3953, 'v3953');
# hash comment 3954
SELECT `mysql_3955` FROM `tbl_5`;
UPDATE bench_t_52 SET payload = 3956 WHERE id = 20;
-- line 3957: deterministic comment
DELETE FROM bench_t_22 WHERE id = 6;
INSERT INTO bench_t_119 (id, payload) VALUES (3959, 'v3959');
-- line 3960: deterministic comment
INSERT INTO bench_t_121 (id, payload) VALUES (3961, 'v3961');
/* block header 3962 */
BEGIN; SELECT 3963; COMMIT;
SELECT 3964 AS id, 'row_3964' AS label;
SELECT [bracket_3965] FROM [dbo].[tbl_5];
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
$dz$ dollar body 3967 ; semicolon inside $dz$
$dz$ dollar body 3968 ; semicolon inside $dz$
/* block header 3969 */
BEGIN; SELECT 3970; COMMIT;
/* block header 3971 */
SELECT [bracket_3972] FROM [dbo].[tbl_12];
UPDATE bench_t_5 SET payload = 3973 WHERE id = 5;
SELECT * FROM "quoted_3974" WHERE col = E'esc\'3974';
BEGIN; SELECT 3975; COMMIT;
INSERT INTO bench_t_8 (id, payload) VALUES (3976, 'v3976');
SELECT nested FROM t WHERE id IN (3977, 3978, 3979);
UPDATE bench_t_10 SET payload = 3978 WHERE id = 10;
SELECT [bracket_3979] FROM [dbo].[tbl_19];
BEGIN; SELECT 3980; COMMIT;
$dz$ dollar body 3981 ; semicolon inside $dz$
SELECT [bracket_3982] FROM [dbo].[tbl_22];
BEGIN; SELECT 3983; COMMIT;
SELECT 3984 AS id, 'row_3984' AS label;
INSERT INTO bench_t_17 (id, payload) VALUES (3985, 'v3985');
SELECT * FROM "quoted_3986" WHERE col = E'esc\'3986';
SELECT nested FROM t WHERE id IN (3987, 3988, 3989);
SELECT * FROM "quoted_3988" WHERE col = E'esc\'3988';
# hash comment 3989
-- line 3990: deterministic comment
SELECT 3991 AS id, 'row_3991' AS label;
BEGIN; SELECT 3992; COMMIT;
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT 3994 AS id, 'row_3994' AS label;
SELECT [bracket_3995] FROM [dbo].[tbl_35];
UPDATE bench_t_28 SET payload = 3996 WHERE id = 28;
UPDATE bench_t_29 SET payload = 3997 WHERE id = 29;
$dz$ dollar body 3998 ; semicolon inside $dz$
SELECT `mysql_3999` FROM `tbl_49`;
/*
 * section 16
 * checksum 9885
 */
$dz$ dollar body 4000 ; semicolon inside $dz$
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
BEGIN; SELECT 4006; COMMIT;
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
/* block header 4008 */
WITH cte_4009 AS (SELECT 4009 AS n) SELECT n FROM cte_4009;
/* block header 4010 */
UPDATE bench_t_43 SET payload = 4011 WHERE id = 11;
# hash comment 4012
$dz$ dollar body 4013 ; semicolon inside $dz$
DELETE FROM bench_t_14 WHERE id = 14;
UPDATE bench_t_47 SET payload = 4015 WHERE id = 15;
UPDATE bench_t_48 SET payload = 4016 WHERE id = 16;
SELECT nested FROM t WHERE id IN (4017, 4018, 4019);
SELECT `mysql_4018` FROM `tbl_18`;
/* block header 4019 */
UPDATE bench_t_52 SET payload = 4020 WHERE id = 20;
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT nested FROM t WHERE id IN (4022, 4023, 4024);
WITH cte_4023 AS (SELECT 4023 AS n) SELECT n FROM cte_4023;
INSERT INTO bench_t_56 (id, payload) VALUES (4024, 'v4024');
-- line 4025: deterministic comment
SELECT nested FROM t WHERE id IN (4026, 4027, 4028);
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
BEGIN; SELECT 4028; COMMIT;
SELECT `mysql_4029` FROM `tbl_29`;
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
# hash comment 4031
SELECT * FROM "quoted_4032" WHERE col = E'esc\'4032';
BEGIN; SELECT 4033; COMMIT;
SELECT [bracket_4034] FROM [dbo].[tbl_34];
$dz$ dollar body 4035 ; semicolon inside $dz$
WITH cte_4036 AS (SELECT 4036 AS n) SELECT n FROM cte_4036;
SELECT nested FROM t WHERE id IN (4037, 4038, 4039);
DELETE FROM bench_t_6 WHERE id = 6;
# hash comment 4039
SELECT 4040 AS id, 'row_4040' AS label;
-- line 4041: deterministic comment
# hash comment 4042
UPDATE bench_t_11 SET payload = 4043 WHERE id = 11;
SELECT `mysql_4044` FROM `tbl_44`;
DELETE FROM bench_t_13 WHERE id = 13;
UPDATE bench_t_14 SET payload = 4046 WHERE id = 14;
DELETE FROM bench_t_15 WHERE id = 15;
# hash comment 4048
BEGIN; SELECT 4049; COMMIT;
DELETE FROM bench_t_18 WHERE id = 2;
INSERT INTO bench_t_83 (id, payload) VALUES (4051, 'v4051');
UPDATE bench_t_20 SET payload = 4052 WHERE id = 20;
SELECT `mysql_4053` FROM `tbl_3`;
INSERT INTO bench_t_86 (id, payload) VALUES (4054, 'v4054');
BEGIN; SELECT 4055; COMMIT;
INSERT INTO bench_t_88 (id, payload) VALUES (4056, 'v4056');
SELECT [bracket_4057] FROM [dbo].[tbl_17];
SELECT [bracket_4058] FROM [dbo].[tbl_18];
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT `mysql_4060` FROM `tbl_10`;
SELECT 4061 AS id, 'row_4061' AS label;
$dz$ dollar body 4062 ; semicolon inside $dz$
SELECT * FROM "quoted_4063" WHERE col = E'esc\'4063';
-- line 4064: deterministic comment
SELECT * FROM "quoted_4065" WHERE col = E'esc\'4065';
-- line 4066: deterministic comment
SELECT `mysql_4067` FROM `tbl_17`;
DELETE FROM bench_t_4 WHERE id = 4;
BEGIN; SELECT 4069; COMMIT;
$dz$ dollar body 4070 ; semicolon inside $dz$
# hash comment 4071
SELECT [bracket_4072] FROM [dbo].[tbl_32];
UPDATE bench_t_41 SET payload = 4073 WHERE id = 9;
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT 4075 AS id, 'row_4075' AS label;
BEGIN; SELECT 4076; COMMIT;
DELETE FROM bench_t_13 WHERE id = 13;
UPDATE bench_t_46 SET payload = 4078 WHERE id = 14;
-- line 4079: deterministic comment
SELECT 4080 AS id, 'row_4080' AS label;
INSERT INTO bench_t_113 (id, payload) VALUES (4081, 'O''Brien');
SELECT nested FROM t WHERE id IN (4082, 4083, 4084);
BEGIN; SELECT 4083; COMMIT;
SELECT [bracket_4084] FROM [dbo].[tbl_4];
SELECT [bracket_4085] FROM [dbo].[tbl_5];
UPDATE bench_t_54 SET payload = 4086 WHERE id = 22;
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT * FROM "quoted_4088" WHERE col = E'esc\'4088';
SELECT [bracket_4089] FROM [dbo].[tbl_9];
/* block header 4090 */
# hash comment 4091
$dz$ dollar body 4092 ; semicolon inside $dz$
WITH cte_4093 AS (SELECT 4093 AS n) SELECT n FROM cte_4093;
UPDATE bench_t_62 SET payload = 4094 WHERE id = 30;
# hash comment 4095
SELECT * FROM "quoted_4096" WHERE col = E'esc\'4096';
-- line 4097: deterministic comment
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
INSERT INTO bench_t_3 (id, payload) VALUES (4099, 'v4099');
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
-- line 4101: deterministic comment
-- line 4102: deterministic comment
/* block header 4103 */
WITH cte_4104 AS (SELECT 4104 AS n) SELECT n FROM cte_4104;
SELECT 4105 AS id, 'row_4105' AS label;
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
DELETE FROM bench_t_11 WHERE id = 11;
UPDATE bench_t_12 SET payload = 4108 WHERE id = 12;
# hash comment 4109
$dz$ dollar body 4110 ; semicolon inside $dz$
DELETE FROM bench_t_15 WHERE id = 15;
SELECT 4112 AS id, 'row_4112' AS label;
SELECT [bracket_4113] FROM [dbo].[tbl_33];
BEGIN; SELECT 4114; COMMIT;
DELETE FROM bench_t_19 WHERE id = 3;
SELECT `mysql_4116` FROM `tbl_16`;
$dz$ dollar body 4117 ; semicolon inside $dz$
SELECT [bracket_4118] FROM [dbo].[tbl_38];
INSERT INTO bench_t_23 (id, payload) VALUES (4119, 'v4119');
# hash comment 4120
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
# hash comment 4122
BEGIN; SELECT 4123; COMMIT;
SELECT [bracket_4124] FROM [dbo].[tbl_4];
BEGIN; SELECT 4125; COMMIT;
$dz$ dollar body 4126 ; semicolon inside $dz$
WITH cte_4127 AS (SELECT 4127 AS n) SELECT n FROM cte_4127;
# hash comment 4128
BEGIN; SELECT 4129; COMMIT;
UPDATE bench_t_34 SET payload = 4130 WHERE id = 2;
INSERT INTO bench_t_35 (id, payload) VALUES (4131, 'v4131');
WITH cte_4132 AS (SELECT 4132 AS n) SELECT n FROM cte_4132;
$dz$ dollar body 4133 ; semicolon inside $dz$
WITH cte_4134 AS (SELECT 4134 AS n) SELECT n FROM cte_4134;
BEGIN; SELECT 4135; COMMIT;
BEGIN; SELECT 4136; COMMIT;
$dz$ dollar body 4137 ; semicolon inside $dz$
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT [bracket_4139] FROM [dbo].[tbl_19];
INSERT INTO bench_t_44 (id, payload) VALUES (4140, 'v4140');
SELECT `mysql_4141` FROM `tbl_41`;
# hash comment 4142
DELETE FROM bench_t_15 WHERE id = 15;
# hash comment 4144
BEGIN; SELECT 4145; COMMIT;
/* block header 4146 */
WITH cte_4147 AS (SELECT 4147 AS n) SELECT n FROM cte_4147;
SELECT nested FROM t WHERE id IN (4148, 4149, 4150);
SELECT [bracket_4149] FROM [dbo].[tbl_29];
UPDATE bench_t_54 SET payload = 4150 WHERE id = 22;
-- line 4151: deterministic comment
BEGIN; SELECT 4152; COMMIT;
SELECT [bracket_4153] FROM [dbo].[tbl_33];
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
/* block header 4156 */
$dz$ dollar body 4157 ; semicolon inside $dz$
SELECT [bracket_4158] FROM [dbo].[tbl_38];
BEGIN; SELECT 4159; COMMIT;
SELECT * FROM "quoted_4160" WHERE col = E'esc\'4160';
SELECT `mysql_4161` FROM `tbl_11`;
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
/* block header 4163 */
UPDATE bench_t_4 SET payload = 4164 WHERE id = 4;
WITH cte_4165 AS (SELECT 4165 AS n) SELECT n FROM cte_4165;
$dz$ dollar body 4166 ; semicolon inside $dz$
-- line 4167: deterministic comment
SELECT `mysql_4168` FROM `tbl_18`;
BEGIN; SELECT 4169; COMMIT;
-- line 4170: deterministic comment
INSERT INTO bench_t_75 (id, payload) VALUES (4171, 'v4171');
UPDATE bench_t_12 SET payload = 4172 WHERE id = 12;
DELETE FROM bench_t_13 WHERE id = 13;
SELECT 4174 AS id, 'row_4174' AS label;
UPDATE bench_t_15 SET payload = 4175 WHERE id = 15;
DELETE FROM bench_t_16 WHERE id = 0;
$dz$ dollar body 4177 ; semicolon inside $dz$
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT [bracket_4179] FROM [dbo].[tbl_19];
# hash comment 4180
BEGIN; SELECT 4181; COMMIT;
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
UPDATE bench_t_23 SET payload = 4183 WHERE id = 23;
SELECT nested FROM t WHERE id IN (4184, 4185, 4186);
BEGIN; SELECT 4185; COMMIT;
SELECT [bracket_4186] FROM [dbo].[tbl_26];
# hash comment 4187
BEGIN; SELECT 4188; COMMIT;
UPDATE bench_t_29 SET payload = 4189 WHERE id = 29;
WITH cte_4190 AS (SELECT 4190 AS n) SELECT n FROM cte_4190;
$dz$ dollar body 4191 ; semicolon inside $dz$
SELECT `mysql_4192` FROM `tbl_42`;
INSERT INTO bench_t_97 (id, payload) VALUES (4193, 'v4193');
BEGIN; SELECT 4194; COMMIT;
-- line 4195: deterministic comment
-- line 4196: deterministic comment
WITH cte_4197 AS (SELECT 4197 AS n) SELECT n FROM cte_4197;
$dz$ dollar body 4198 ; semicolon inside $dz$
SELECT [bracket_4199] FROM [dbo].[tbl_39];
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
DELETE FROM bench_t_11 WHERE id = 11;
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
UPDATE bench_t_45 SET payload = 4205 WHERE id = 13;
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
UPDATE bench_t_47 SET payload = 4207 WHERE id = 15;
$dz$ dollar body 4208 ; semicolon inside $dz$
BEGIN; SELECT 4209; COMMIT;
WITH cte_4210 AS (SELECT 4210 AS n) SELECT n FROM cte_4210;
BEGIN; SELECT 4211; COMMIT;
SELECT * FROM "quoted_4212" WHERE col = E'esc\'4212';
$dz$ dollar body 4213 ; semicolon inside $dz$
-- line 4214: deterministic comment
SELECT nested FROM t WHERE id IN (4215, 4216, 4217);
UPDATE bench_t_56 SET payload = 4216 WHERE id = 24;
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT * FROM "quoted_4219" WHERE col = E'esc\'4219';
SELECT [bracket_4220] FROM [dbo].[tbl_20];
SELECT `mysql_4221` FROM `tbl_21`;
# hash comment 4222
$dz$ dollar body 4223 ; semicolon inside $dz$
DELETE FROM bench_t_0 WHERE id = 0;
BEGIN; SELECT 4225; COMMIT;
DELETE FROM bench_t_2 WHERE id = 2;
DELETE FROM bench_t_3 WHERE id = 3;
SELECT `mysql_4228` FROM `tbl_28`;
SELECT * FROM "quoted_4229" WHERE col = E'esc\'4229';
SELECT [bracket_4230] FROM [dbo].[tbl_30];
SELECT `mysql_4231` FROM `tbl_31`;
SELECT nested FROM t WHERE id IN (4232, 4233, 4234);
UPDATE bench_t_9 SET payload = 4233 WHERE id = 9;
DELETE FROM bench_t_10 WHERE id = 10;
-- line 4235: deterministic comment
$dz$ dollar body 4236 ; semicolon inside $dz$
-- line 4237: deterministic comment
WITH cte_4238 AS (SELECT 4238 AS n) SELECT n FROM cte_4238;
WITH cte_4239 AS (SELECT 4239 AS n) SELECT n FROM cte_4239;
$dz$ dollar body 4240 ; semicolon inside $dz$
WITH cte_4241 AS (SELECT 4241 AS n) SELECT n FROM cte_4241;
UPDATE bench_t_18 SET payload = 4242 WHERE id = 18;
/* block header 4243 */
SELECT * FROM "quoted_4244" WHERE col = E'esc\'4244';
SELECT nested FROM t WHERE id IN (4245, 4246, 4247);
SELECT 4246 AS id, 'row_4246' AS label;
UPDATE bench_t_23 SET payload = 4247 WHERE id = 23;
/* block header 4248 */
SELECT `mysql_4249` FROM `tbl_49`;
/*
 * section 17
 * checksum 175d
 */
SELECT 4250 AS id, 'row_4250' AS label;
BEGIN; SELECT 4255; COMMIT;
# hash comment 4256
INSERT INTO bench_t_33 (id, payload) VALUES (4257, 'O''Brien');
/* block header 4258 */
UPDATE bench_t_35 SET payload = 4259 WHERE id = 3;
WITH cte_4260 AS (SELECT 4260 AS n) SELECT n FROM cte_4260;
BEGIN; SELECT 4261; COMMIT;
$dz$ dollar body 4262 ; semicolon inside $dz$
$dz$ dollar body 4263 ; semicolon inside $dz$
UPDATE bench_t_40 SET payload = 4264 WHERE id = 8;
SELECT 4265 AS id, 'row_4265' AS label;
/* block header 4266 */
SELECT [bracket_4267] FROM [dbo].[tbl_27];
# hash comment 4268
DELETE FROM bench_t_13 WHERE id = 13;
SELECT 4270 AS id, 'row_4270' AS label;
# hash comment 4271
DELETE FROM bench_t_16 WHERE id = 0;
-- line 4273: deterministic comment
SELECT nested FROM t WHERE id IN (4274, 4275, 4276);
SELECT `mysql_4275` FROM `tbl_25`;
SELECT `mysql_4276` FROM `tbl_26`;
# hash comment 4277
WITH cte_4278 AS (SELECT 4278 AS n) SELECT n FROM cte_4278;
SELECT `mysql_4279` FROM `tbl_29`;
SELECT [bracket_4280] FROM [dbo].[tbl_0];
WITH cte_4281 AS (SELECT 4281 AS n) SELECT n FROM cte_4281;
DELETE FROM bench_t_26 WHERE id = 10;
SELECT 4283 AS id, 'row_4283' AS label;
UPDATE bench_t_60 SET payload = 4284 WHERE id = 28;
SELECT * FROM "quoted_4285" WHERE col = E'esc\'4285';
# hash comment 4286
INSERT INTO bench_t_63 (id, payload) VALUES (4287, 'v4287');
WITH cte_4288 AS (SELECT 4288 AS n) SELECT n FROM cte_4288;
$dz$ dollar body 4289 ; semicolon inside $dz$
UPDATE bench_t_2 SET payload = 4290 WHERE id = 2;
SELECT [bracket_4291] FROM [dbo].[tbl_11];
SELECT `mysql_4292` FROM `tbl_42`;
$dz$ dollar body 4293 ; semicolon inside $dz$
-- line 4294: deterministic comment
$dz$ dollar body 4295 ; semicolon inside $dz$
-- line 4296: deterministic comment
# hash comment 4297
WITH cte_4298 AS (SELECT 4298 AS n) SELECT n FROM cte_4298;
# hash comment 4299
BEGIN; SELECT 4300; COMMIT;
SELECT * FROM "quoted_4301" WHERE col = E'esc\'4301';
INSERT INTO bench_t_78 (id, payload) VALUES (4302, 'v4302');
# hash comment 4303
BEGIN; SELECT 4304; COMMIT;
DELETE FROM bench_t_17 WHERE id = 1;
SELECT * FROM "quoted_4306" WHERE col = E'esc\'4306';
INSERT INTO bench_t_83 (id, payload) VALUES (4307, 'v4307');
SELECT [bracket_4308] FROM [dbo].[tbl_28];
SELECT 4309 AS id, 'row_4309' AS label;
$dz$ dollar body 4310 ; semicolon inside $dz$
# hash comment 4311
BEGIN; SELECT 4312; COMMIT;
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
UPDATE bench_t_27 SET payload = 4315 WHERE id = 27;
-- line 4316: deterministic comment
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT `mysql_4318` FROM `tbl_18`;
-- line 4319: deterministic comment
SELECT `mysql_4320` FROM `tbl_20`;
BEGIN; SELECT 4321; COMMIT;
SELECT * FROM "quoted_4322" WHERE col = E'esc\'4322';
SELECT [bracket_4323] FROM [dbo].[tbl_3];
SELECT nested FROM t WHERE id IN (4324, 4325, 4326);
SELECT [bracket_4325] FROM [dbo].[tbl_5];
# hash comment 4326
-- line 4327: deterministic comment
# hash comment 4328
SELECT 4329 AS id, 'row_4329' AS label;
SELECT [bracket_4330] FROM [dbo].[tbl_10];
DELETE FROM bench_t_11 WHERE id = 11;
DELETE FROM bench_t_12 WHERE id = 12;
SELECT `mysql_4333` FROM `tbl_33`;
SELECT nested FROM t WHERE id IN (4334, 4335, 4336);
-- line 4335: deterministic comment
SELECT `mysql_4336` FROM `tbl_36`;
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
# hash comment 4338
-- line 4339: deterministic comment
BEGIN; SELECT 4340; COMMIT;
SELECT * FROM "quoted_4341" WHERE col = E'esc\'4341';
SELECT 4342 AS id, 'row_4342' AS label;
$dz$ dollar body 4343 ; semicolon inside $dz$
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
# hash comment 4345
SELECT [bracket_4346] FROM [dbo].[tbl_26];
SELECT * FROM "quoted_4347" WHERE col = E'esc\'4347';
DELETE FROM bench_t_28 WHERE id = 12;
INSERT INTO bench_t_125 (id, payload) VALUES (4349, 'v4349');
DELETE FROM bench_t_30 WHERE id = 14;
# hash comment 4351
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT [bracket_4355] FROM [dbo].[tbl_35];
UPDATE bench_t_4 SET payload = 4356 WHERE id = 4;
# hash comment 4357
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT `mysql_4359` FROM `tbl_9`;
SELECT `mysql_4360` FROM `tbl_10`;
DELETE FROM bench_t_9 WHERE id = 9;
-- line 4362: deterministic comment
DELETE FROM bench_t_11 WHERE id = 11;
SELECT 4364 AS id, 'row_4364' AS label;
SELECT * FROM "quoted_4365" WHERE col = E'esc\'4365';
SELECT * FROM "quoted_4366" WHERE col = E'esc\'4366';
SELECT `mysql_4367` FROM `tbl_17`;
$dz$ dollar body 4368 ; semicolon inside $dz$
$dz$ dollar body 4369 ; semicolon inside $dz$
$dz$ dollar body 4370 ; semicolon inside $dz$
# hash comment 4371
INSERT INTO bench_t_20 (id, payload) VALUES (4372, 'v4372');
BEGIN; SELECT 4373; COMMIT;
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
WITH cte_4375 AS (SELECT 4375 AS n) SELECT n FROM cte_4375;
SELECT 4376 AS id, 'row_4376' AS label;
SELECT [bracket_4377] FROM [dbo].[tbl_17];
SELECT * FROM "quoted_4378" WHERE col = E'esc\'4378';
SELECT [bracket_4379] FROM [dbo].[tbl_19];
SELECT [bracket_4380] FROM [dbo].[tbl_20];
/* block header 4381 */
SELECT nested FROM t WHERE id IN (4382, 4383, 4384);
SELECT nested FROM t WHERE id IN (4383, 4384, 4385);
WITH cte_4384 AS (SELECT 4384 AS n) SELECT n FROM cte_4384;
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
# hash comment 4386
BEGIN; SELECT 4387; COMMIT;
$dz$ dollar body 4388 ; semicolon inside $dz$
-- line 4389: deterministic comment
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
-- line 4391: deterministic comment
-- line 4392: deterministic comment
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
DELETE FROM bench_t_11 WHERE id = 11;
SELECT 4396 AS id, 'row_4396' AS label;
SELECT * FROM "quoted_4397" WHERE col = E'esc\'4397';
BEGIN; SELECT 4398; COMMIT;
SELECT 4399 AS id, 'row_4399' AS label;
# hash comment 4400
$dz$ dollar body 4401 ; semicolon inside $dz$
$dz$ dollar body 4402 ; semicolon inside $dz$
# hash comment 4403
SELECT nested FROM t WHERE id IN (4404, 4405, 4406);
SELECT * FROM "quoted_4405" WHERE col = E'esc\'4405';
SELECT `mysql_4406` FROM `tbl_6`;
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
$dz$ dollar body 4408 ; semicolon inside $dz$
INSERT INTO bench_t_57 (id, payload) VALUES (4409, 'v4409');
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT `mysql_4411` FROM `tbl_11`;
$dz$ dollar body 4412 ; semicolon inside $dz$
BEGIN; SELECT 4413; COMMIT;
SELECT `mysql_4414` FROM `tbl_14`;
INSERT INTO bench_t_63 (id, payload) VALUES (4415, 'v4415');
WITH cte_4416 AS (SELECT 4416 AS n) SELECT n FROM cte_4416;
INSERT INTO bench_t_65 (id, payload) VALUES (4417, 'v4417');
SELECT 4418 AS id, 'row_4418' AS label;
SELECT 4419 AS id, 'row_4419' AS label;
# hash comment 4420
SELECT `mysql_4421` FROM `tbl_21`;
/* block header 4422 */
SELECT nested FROM t WHERE id IN (4423, 4424, 4425);
/* block header 4424 */
SELECT `mysql_4425` FROM `tbl_25`;
INSERT INTO bench_t_74 (id, payload) VALUES (4426, 'v4426');
$dz$ dollar body 4427 ; semicolon inside $dz$
BEGIN; SELECT 4428; COMMIT;
WITH cte_4429 AS (SELECT 4429 AS n) SELECT n FROM cte_4429;
SELECT `mysql_4430` FROM `tbl_30`;
SELECT [bracket_4431] FROM [dbo].[tbl_31];
WITH cte_4432 AS (SELECT 4432 AS n) SELECT n FROM cte_4432;
BEGIN; SELECT 4433; COMMIT;
SELECT [bracket_4434] FROM [dbo].[tbl_34];
/* block header 4435 */
BEGIN; SELECT 4436; COMMIT;
/* block header 4437 */
$dz$ dollar body 4438 ; semicolon inside $dz$
-- line 4439: deterministic comment
SELECT * FROM "quoted_4440" WHERE col = E'esc\'4440';
INSERT INTO bench_t_89 (id, payload) VALUES (4441, 'v4441');
DELETE FROM bench_t_26 WHERE id = 10;
UPDATE bench_t_27 SET payload = 4443 WHERE id = 27;
UPDATE bench_t_28 SET payload = 4444 WHERE id = 28;
UPDATE bench_t_29 SET payload = 4445 WHERE id = 29;
INSERT INTO bench_t_94 (id, payload) VALUES (4446, 'v4446');
$dz$ dollar body 4447 ; semicolon inside $dz$
SELECT nested FROM t WHERE id IN (4448, 4449, 4450);
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
-- line 4450: deterministic comment
DELETE FROM bench_t_3 WHERE id = 3;
-- line 4452: deterministic comment
/* block header 4453 */
WITH cte_4454 AS (SELECT 4454 AS n) SELECT n FROM cte_4454;
$dz$ dollar body 4455 ; semicolon inside $dz$
# hash comment 4456
SELECT [bracket_4457] FROM [dbo].[tbl_17];
/* block header 4458 */
SELECT nested FROM t WHERE id IN (4459, 4460, 4461);
SELECT 4460 AS id, 'row_4460' AS label;
$dz$ dollar body 4461 ; semicolon inside $dz$
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
INSERT INTO bench_t_111 (id, payload) VALUES (4463, 'v4463');
$dz$ dollar body 4464 ; semicolon inside $dz$
/* block header 4465 */
UPDATE bench_t_50 SET payload = 4466 WHERE id = 18;
BEGIN; SELECT 4467; COMMIT;
DELETE FROM bench_t_20 WHERE id = 4;
SELECT 4469 AS id, 'row_4469' AS label;
$dz$ dollar body 4470 ; semicolon inside $dz$
BEGIN; SELECT 4471; COMMIT;
BEGIN; SELECT 4472; COMMIT;
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
$dz$ dollar body 4474 ; semicolon inside $dz$
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT nested FROM t WHERE id IN (4476, 4477, 4478);
WITH cte_4477 AS (SELECT 4477 AS n) SELECT n FROM cte_4477;
SELECT 4478 AS id, 'row_4478' AS label;
SELECT [bracket_4479] FROM [dbo].[tbl_39];
SELECT `mysql_4480` FROM `tbl_30`;
SELECT * FROM "quoted_4481" WHERE col = E'esc\'4481';
SELECT * FROM "quoted_4482" WHERE col = E'esc\'4482';
/* block header 4483 */
WITH cte_4484 AS (SELECT 4484 AS n) SELECT n FROM cte_4484;
SELECT [bracket_4485] FROM [dbo].[tbl_5];
UPDATE bench_t_6 SET payload = 4486 WHERE id = 6;
SELECT 4487 AS id, 'row_4487' AS label;
SELECT `mysql_4488` FROM `tbl_38`;
# hash comment 4489
/* block header 4490 */
DELETE FROM bench_t_11 WHERE id = 11;
DELETE FROM bench_t_12 WHERE id = 12;
# hash comment 4493
SELECT `mysql_4494` FROM `tbl_44`;
UPDATE bench_t_15 SET payload = 4495 WHERE id = 15;
SELECT 4496 AS id, 'row_4496' AS label;
INSERT INTO bench_t_17 (id, payload) VALUES (4497, 'v4497');
# hash comment 4498
UPDATE bench_t_19 SET payload = 4499 WHERE id = 19;
/*
 * section 18
 * checksum 3ac3
 */
DELETE FROM bench_t_20 WHERE id = 4;
SELECT `mysql_4505` FROM `tbl_5`;
SELECT 4506 AS id, 'row_4506' AS label;
/* block header 4507 */
BEGIN; SELECT 4508; COMMIT;
WITH cte_4509 AS (SELECT 4509 AS n) SELECT n FROM cte_4509;
SELECT 4510 AS id, 'row_4510' AS label;
$dz$ dollar body 4511 ; semicolon inside $dz$
INSERT INTO bench_t_32 (id, payload) VALUES (4512, 'v4512');
SELECT nested FROM t WHERE id IN (4513, 4514, 4515);
SELECT * FROM "quoted_4514" WHERE col = E'esc\'4514';
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT `mysql_4516` FROM `tbl_16`;
SELECT nested FROM t WHERE id IN (4517, 4518, 4519);
SELECT nested FROM t WHERE id IN (4518, 4519, 4520);
SELECT `mysql_4519` FROM `tbl_19`;
SELECT nested FROM t WHERE id IN (4520, 4521, 4522);
UPDATE bench_t_41 SET payload = 4521 WHERE id = 9;
INSERT INTO bench_t_42 (id, payload) VALUES (4522, 'v4522');
UPDATE bench_t_43 SET payload = 4523 WHERE id = 11;
/* block header 4524 */
INSERT INTO bench_t_45 (id, payload) VALUES (4525, 'v4525');
WITH cte_4526 AS (SELECT 4526 AS n) SELECT n FROM cte_4526;
SELECT `mysql_4527` FROM `tbl_27`;
# hash comment 4528
SELECT 4529 AS id, 'row_4529' AS label;
$dz$ dollar body 4530 ; semicolon inside $dz$
SELECT `mysql_4531` FROM `tbl_31`;
SELECT `mysql_4532` FROM `tbl_32`;
SELECT 4533 AS id, 'row_4533' AS label;
SELECT `mysql_4534` FROM `tbl_34`;
DELETE FROM bench_t_23 WHERE id = 7;
SELECT nested FROM t WHERE id IN (4536, 4537, 4538);
DELETE FROM bench_t_25 WHERE id = 9;
SELECT `mysql_4538` FROM `tbl_38`;
$dz$ dollar body 4539 ; semicolon inside $dz$
# hash comment 4540
-- line 4541: deterministic comment
-- line 4542: deterministic comment
INSERT INTO bench_t_63 (id, payload) VALUES (4543, 'O''Brien');
WITH cte_4544 AS (SELECT 4544 AS n) SELECT n FROM cte_4544;
INSERT INTO bench_t_65 (id, payload) VALUES (4545, 'v4545');
INSERT INTO bench_t_66 (id, payload) VALUES (4546, 'v4546');
WITH cte_4547 AS (SELECT 4547 AS n) SELECT n FROM cte_4547;
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
DELETE FROM bench_t_5 WHERE id = 5;
-- line 4550: deterministic comment
-- line 4551: deterministic comment
BEGIN; SELECT 4552; COMMIT;
UPDATE bench_t_9 SET payload = 4553 WHERE id = 9;
DELETE FROM bench_t_10 WHERE id = 10;
SELECT * FROM "quoted_4555" WHERE col = E'esc\'4555';
INSERT INTO bench_t_76 (id, payload) VALUES (4556, 'v4556');
-- line 4557: deterministic comment
INSERT INTO bench_t_78 (id, payload) VALUES (4558, 'v4558');
BEGIN; SELECT 4559; COMMIT;
SELECT [bracket_4560] FROM [dbo].[tbl_0];
SELECT nested FROM t WHERE id IN (4561, 4562, 4563);
-- line 4562: deterministic comment
SELECT 4563 AS id, 'row_4563' AS label;
$dz$ dollar body 4564 ; semicolon inside $dz$
UPDATE bench_t_21 SET payload = 4565 WHERE id = 21;
WITH cte_4566 AS (SELECT 4566 AS n) SELECT n FROM cte_4566;
SELECT 4567 AS id, 'row_4567' AS label;
BEGIN; SELECT 4568; COMMIT;
SELECT nested FROM t WHERE id IN (4569, 4570, 4571);
DELETE FROM bench_t_26 WHERE id = 10;
SELECT 4571 AS id, 'row_4571' AS label;
INSERT INTO bench_t_92 (id, payload) VALUES (4572, 'v4572');
SELECT nested FROM t WHERE id IN (4573, 4574, 4575);
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
INSERT INTO bench_t_95 (id, payload) VALUES (4575, 'v4575');
DELETE FROM bench_t_0 WHERE id = 0;
$dz$ dollar body 4577 ; semicolon inside $dz$
# hash comment 4578
SELECT * FROM "quoted_4579" WHERE col = E'esc\'4579';
SELECT `mysql_4580` FROM `tbl_30`;
$dz$ dollar body 4581 ; semicolon inside $dz$
BEGIN; SELECT 4582; COMMIT;
SELECT `mysql_4583` FROM `tbl_33`;
UPDATE bench_t_40 SET payload = 4584 WHERE id = 8;
DELETE FROM bench_t_9 WHERE id = 9;
/* block header 4586 */
SELECT nested FROM t WHERE id IN (4587, 4588, 4589);
$dz$ dollar body 4588 ; semicolon inside $dz$
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
$dz$ dollar body 4590 ; semicolon inside $dz$
SELECT nested FROM t WHERE id IN (4591, 4592, 4593);
BEGIN; SELECT 4592; COMMIT;
-- line 4593: deterministic comment
BEGIN; SELECT 4594; COMMIT;
UPDATE bench_t_51 SET payload = 4595 WHERE id = 19;
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT `mysql_4597` FROM `tbl_47`;
-- line 4598: deterministic comment
DELETE FROM bench_t_23 WHERE id = 7;
$dz$ dollar body 4600 ; semicolon inside $dz$
SELECT nested FROM t WHERE id IN (4601, 4602, 4603);
SELECT 4602 AS id, 'row_4602' AS label;
BEGIN; SELECT 4603; COMMIT;
SELECT nested FROM t WHERE id IN (4604, 4605, 4606);
SELECT 4605 AS id, 'row_4605' AS label;
/* block header 4606 */
# hash comment 4607
SELECT * FROM "quoted_4608" WHERE col = E'esc\'4608';
INSERT INTO bench_t_1 (id, payload) VALUES (4609, 'O''Brien');
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT nested FROM t WHERE id IN (4611, 4612, 4613);
/* block header 4612 */
WITH cte_4613 AS (SELECT 4613 AS n) SELECT n FROM cte_4613;
INSERT INTO bench_t_6 (id, payload) VALUES (4614, 'v4614');
BEGIN; SELECT 4615; COMMIT;
-- line 4616: deterministic comment
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT `mysql_4618` FROM `tbl_18`;
SELECT nested FROM t WHERE id IN (4619, 4620, 4621);
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
INSERT INTO bench_t_13 (id, payload) VALUES (4621, 'v4621');
UPDATE bench_t_14 SET payload = 4622 WHERE id = 14;
SELECT 4623 AS id, 'row_4623' AS label;
-- line 4624: deterministic comment
UPDATE bench_t_17 SET payload = 4625 WHERE id = 17;
SELECT [bracket_4626] FROM [dbo].[tbl_26];
/* block header 4627 */
SELECT * FROM "quoted_4628" WHERE col = E'esc\'4628';
BEGIN; SELECT 4629; COMMIT;
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
UPDATE bench_t_23 SET payload = 4631 WHERE id = 23;
SELECT 4632 AS id, 'row_4632' AS label;
$dz$ dollar body 4633 ; semicolon inside $dz$
SELECT * FROM "quoted_4634" WHERE col = E'esc\'4634';
BEGIN; SELECT 4635; COMMIT;
SELECT [bracket_4636] FROM [dbo].[tbl_36];
# hash comment 4637
SELECT `mysql_4638` FROM `tbl_38`;
-- line 4639: deterministic comment
DELETE FROM bench_t_0 WHERE id = 0;
UPDATE bench_t_33 SET payload = 4641 WHERE id = 1;
SELECT `mysql_4642` FROM `tbl_42`;
SELECT 4643 AS id, 'row_4643' AS label;
WITH cte_4644 AS (SELECT 4644 AS n) SELECT n FROM cte_4644;
SELECT [bracket_4645] FROM [dbo].[tbl_5];
UPDATE bench_t_38 SET payload = 4646 WHERE id = 6;
BEGIN; SELECT 4647; COMMIT;
BEGIN; SELECT 4648; COMMIT;
-- line 4649: deterministic comment
SELECT * FROM "quoted_4650" WHERE col = E'esc\'4650';
UPDATE bench_t_43 SET payload = 4651 WHERE id = 11;
BEGIN; SELECT 4652; COMMIT;
# hash comment 4653
BEGIN; SELECT 4654; COMMIT;
-- line 4655: deterministic comment
/* block header 4656 */
WITH cte_4657 AS (SELECT 4657 AS n) SELECT n FROM cte_4657;
SELECT nested FROM t WHERE id IN (4658, 4659, 4660);
BEGIN; SELECT 4659; COMMIT;
SELECT * FROM "quoted_4660" WHERE col = E'esc\'4660';
SELECT 4661 AS id, 'row_4661' AS label;
/* block header 4662 */
SELECT 4663 AS id, 'row_4663' AS label;
INSERT INTO bench_t_56 (id, payload) VALUES (4664, 'O''Brien');
SELECT [bracket_4665] FROM [dbo].[tbl_25];
SELECT * FROM "quoted_4666" WHERE col = E'esc\'4666';
DELETE FROM bench_t_27 WHERE id = 11;
WITH cte_4668 AS (SELECT 4668 AS n) SELECT n FROM cte_4668;
SELECT 4669 AS id, 'row_4669' AS label;
SELECT [bracket_4670] FROM [dbo].[tbl_30];
DELETE FROM bench_t_31 WHERE id = 15;
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
WITH cte_4673 AS (SELECT 4673 AS n) SELECT n FROM cte_4673;
/* block header 4674 */
BEGIN; SELECT 4675; COMMIT;
SELECT [bracket_4676] FROM [dbo].[tbl_36];
DELETE FROM bench_t_5 WHERE id = 5;
SELECT [bracket_4678] FROM [dbo].[tbl_38];
-- line 4679: deterministic comment
DELETE FROM bench_t_8 WHERE id = 8;
INSERT INTO bench_t_73 (id, payload) VALUES (4681, 'v4681');
UPDATE bench_t_10 SET payload = 4682 WHERE id = 10;
INSERT INTO bench_t_75 (id, payload) VALUES (4683, 'v4683');
$dz$ dollar body 4684 ; semicolon inside $dz$
SELECT `mysql_4685` FROM `tbl_35`;
BEGIN; SELECT 4686; COMMIT;
BEGIN; SELECT 4687; COMMIT;
$dz$ dollar body 4688 ; semicolon inside $dz$
$dz$ dollar body 4689 ; semicolon inside $dz$
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT nested FROM t WHERE id IN (4691, 4692, 4693);
INSERT INTO bench_t_84 (id, payload) VALUES (4692, 'v4692');
UPDATE bench_t_21 SET payload = 4693 WHERE id = 21;
SELECT * FROM "quoted_4694" WHERE col = E'esc\'4694';
/* block header 4695 */
SELECT * FROM "quoted_4696" WHERE col = E'esc\'4696';
WITH cte_4697 AS (SELECT 4697 AS n) SELECT n FROM cte_4697;
DELETE FROM bench_t_26 WHERE id = 10;
# hash comment 4699
# hash comment 4700
SELECT * FROM "quoted_4701" WHERE col = E'esc\'4701';
$dz$ dollar body 4702 ; semicolon inside $dz$
INSERT INTO bench_t_95 (id, payload) VALUES (4703, 'v4703');
INSERT INTO bench_t_96 (id, payload) VALUES (4704, 'v4704');
/* block header 4705 */
SELECT [bracket_4706] FROM [dbo].[tbl_26];
UPDATE bench_t_35 SET payload = 4707 WHERE id = 3;
DELETE FROM bench_t_4 WHERE id = 4;
DELETE FROM bench_t_5 WHERE id = 5;
$dz$ dollar body 4710 ; semicolon inside $dz$
BEGIN; SELECT 4711; COMMIT;
SELECT * FROM "quoted_4712" WHERE col = E'esc\'4712';
$dz$ dollar body 4713 ; semicolon inside $dz$
SELECT nested FROM t WHERE id IN (4714, 4715, 4716);
SELECT * FROM "quoted_4715" WHERE col = E'esc\'4715';
# hash comment 4716
# hash comment 4717
-- line 4718: deterministic comment
# hash comment 4719
-- line 4720: deterministic comment
$dz$ dollar body 4721 ; semicolon inside $dz$
$dz$ dollar body 4722 ; semicolon inside $dz$
-- line 4723: deterministic comment
DELETE FROM bench_t_20 WHERE id = 4;
BEGIN; SELECT 4725; COMMIT;
SELECT nested FROM t WHERE id IN (4726, 4727, 4728);
UPDATE bench_t_55 SET payload = 4727 WHERE id = 23;
BEGIN; SELECT 4728; COMMIT;
/* block header 4729 */
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
$dz$ dollar body 4731 ; semicolon inside $dz$
INSERT INTO bench_t_124 (id, payload) VALUES (4732, 'v4732');
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT [bracket_4734] FROM [dbo].[tbl_14];
$dz$ dollar body 4735 ; semicolon inside $dz$
$dz$ dollar body 4736 ; semicolon inside $dz$
DELETE FROM bench_t_1 WHERE id = 1;
SELECT * FROM "quoted_4738" WHERE col = E'esc\'4738';
-- line 4739: deterministic comment
UPDATE bench_t_4 SET payload = 4740 WHERE id = 4;
WITH cte_4741 AS (SELECT 4741 AS n) SELECT n FROM cte_4741;
BEGIN; SELECT 4742; COMMIT;
# hash comment 4743
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
# hash comment 4745
-- line 4746: deterministic comment
-- line 4747: deterministic comment
SELECT `mysql_4748` FROM `tbl_48`;
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
/*
 * section 19
 * checksum cff7
 */
$dz$ dollar body 4750 ; semicolon inside $dz$
SELECT nested FROM t WHERE id IN (4755, 4756, 4757);
SELECT nested FROM t WHERE id IN (4756, 4757, 4758);
# hash comment 4757
DELETE FROM bench_t_22 WHERE id = 6;
SELECT nested FROM t WHERE id IN (4759, 4760, 4761);
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
UPDATE bench_t_26 SET payload = 4762 WHERE id = 26;
SELECT * FROM "quoted_4763" WHERE col = E'esc\'4763';
SELECT * FROM "quoted_4764" WHERE col = E'esc\'4764';
SELECT * FROM "quoted_4765" WHERE col = E'esc\'4765';
INSERT INTO bench_t_30 (id, payload) VALUES (4766, 'v4766');
INSERT INTO bench_t_31 (id, payload) VALUES (4767, 'v4767');
SELECT nested FROM t WHERE id IN (4768, 4769, 4770);
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
UPDATE bench_t_34 SET payload = 4770 WHERE id = 2;
BEGIN; SELECT 4771; COMMIT;
$dz$ dollar body 4772 ; semicolon inside $dz$
INSERT INTO bench_t_37 (id, payload) VALUES (4773, 'v4773');
# hash comment 4774
# hash comment 4775
SELECT nested FROM t WHERE id IN (4776, 4777, 4778);
/* block header 4777 */
# hash comment 4778
WITH cte_4779 AS (SELECT 4779 AS n) SELECT n FROM cte_4779;
# hash comment 4780
SELECT nested FROM t WHERE id IN (4781, 4782, 4783);
SELECT `mysql_4782` FROM `tbl_32`;
WITH cte_4783 AS (SELECT 4783 AS n) SELECT n FROM cte_4783;
/* block header 4784 */
SELECT [bracket_4785] FROM [dbo].[tbl_25];
$dz$ dollar body 4786 ; semicolon inside $dz$
/* block header 4787 */
$dz$ dollar body 4788 ; semicolon inside $dz$
SELECT * FROM "quoted_4789" WHERE col = E'esc\'4789';
SELECT nested FROM t WHERE id IN (4790, 4791, 4792);
DELETE FROM bench_t_23 WHERE id = 7;
INSERT INTO bench_t_56 (id, payload) VALUES (4792, 'v4792');
SELECT * FROM "quoted_4793" WHERE col = E'esc\'4793';
INSERT INTO bench_t_58 (id, payload) VALUES (4794, 'v4794');
UPDATE bench_t_59 SET payload = 4795 WHERE id = 27;
SELECT * FROM "quoted_4796" WHERE col = E'esc\'4796';
DELETE FROM bench_t_29 WHERE id = 13;
WITH cte_4798 AS (SELECT 4798 AS n) SELECT n FROM cte_4798;
# hash comment 4799
SELECT nested FROM t WHERE id IN (4800, 4801, 4802);
BEGIN; SELECT 4801; COMMIT;
$dz$ dollar body 4802 ; semicolon inside $dz$
UPDATE bench_t_3 SET payload = 4803 WHERE id = 3;
UPDATE bench_t_4 SET payload = 4804 WHERE id = 4;
-- line 4805: deterministic comment
SELECT * FROM "quoted_4806" WHERE col = E'esc\'4806';
SELECT nested FROM t WHERE id IN (4807, 4808, 4809);
INSERT INTO bench_t_72 (id, payload) VALUES (4808, 'v4808');
# hash comment 4809
DELETE FROM bench_t_10 WHERE id = 10;
INSERT INTO bench_t_75 (id, payload) VALUES (4811, 'v4811');
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
BEGIN; SELECT 4813; COMMIT;
BEGIN; SELECT 4814; COMMIT;
INSERT INTO bench_t_79 (id, payload) VALUES (4815, 'v4815');
/* block header 4816 */
/* block header 4817 */
SELECT nested FROM t WHERE id IN (4818, 4819, 4820);
WITH cte_4819 AS (SELECT 4819 AS n) SELECT n FROM cte_4819;
$dz$ dollar body 4820 ; semicolon inside $dz$
INSERT INTO bench_t_85 (id, payload) VALUES (4821, 'v4821');
$dz$ dollar body 4822 ; semicolon inside $dz$
SELECT [bracket_4823] FROM [dbo].[tbl_23];
/* block header 4824 */
SELECT 4825 AS id, 'row_4825' AS label;
SELECT * FROM "quoted_4826" WHERE col = E'esc\'4826';
$dz$ dollar body 4827 ; semicolon inside $dz$
DELETE FROM bench_t_28 WHERE id = 12;
BEGIN; SELECT 4829; COMMIT;
SELECT [bracket_4830] FROM [dbo].[tbl_30];
SELECT 4831 AS id, 'row_4831' AS label;
WITH cte_4832 AS (SELECT 4832 AS n) SELECT n FROM cte_4832;
WITH cte_4833 AS (SELECT 4833 AS n) SELECT n FROM cte_4833;
# hash comment 4834
-- line 4835: deterministic comment
SELECT * FROM "quoted_4836" WHERE col = E'esc\'4836';
SELECT nested FROM t WHERE id IN (4837, 4838, 4839);
$dz$ dollar body 4838 ; semicolon inside $dz$
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
DELETE FROM bench_t_8 WHERE id = 8;
DELETE FROM bench_t_9 WHERE id = 9;
SELECT 4842 AS id, 'row_4842' AS label;
BEGIN; SELECT 4843; COMMIT;
DELETE FROM bench_t_12 WHERE id = 12;
BEGIN; SELECT 4845; COMMIT;
SELECT nested FROM t WHERE id IN (4846, 4847, 4848);
WITH cte_4847 AS (SELECT 4847 AS n) SELECT n FROM cte_4847;
WITH cte_4848 AS (SELECT 4848 AS n) SELECT n FROM cte_4848;
SELECT * FROM "quoted_4849" WHERE col = E'esc\'4849';
# hash comment 4850
$dz$ dollar body 4851 ; semicolon inside $dz$
WITH cte_4852 AS (SELECT 4852 AS n) SELECT n FROM cte_4852;
# hash comment 4853
SELECT `mysql_4854` FROM `tbl_4`;
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT * FROM "quoted_4856" WHERE col = E'esc\'4856';
/* block header 4857 */
UPDATE bench_t_58 SET payload = 4858 WHERE id = 26;
-- line 4859: deterministic comment
WITH cte_4860 AS (SELECT 4860 AS n) SELECT n FROM cte_4860;
# hash comment 4861
# hash comment 4862
BEGIN; SELECT 4863; COMMIT;
$dz$ dollar body 4864 ; semicolon inside $dz$
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
DELETE FROM bench_t_3 WHERE id = 3;
SELECT nested FROM t WHERE id IN (4868, 4869, 4870);
SELECT [bracket_4869] FROM [dbo].[tbl_29];
BEGIN; SELECT 4870; COMMIT;
/* block header 4871 */
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT `mysql_4873` FROM `tbl_23`;
SELECT nested FROM t WHERE id IN (4874, 4875, 4876);
SELECT 4875 AS id, 'row_4875' AS label;
SELECT `mysql_4876` FROM `tbl_26`;
# hash comment 4877
/* block header 4878 */
# hash comment 4879
SELECT 4880 AS id, 'row_4880' AS label;
WITH cte_4881 AS (SELECT 4881 AS n) SELECT n FROM cte_4881;
SELECT nested FROM t WHERE id IN (4882, 4883, 4884);
SELECT 4883 AS id, 'row_4883' AS label;
DELETE FROM bench_t_20 WHERE id = 4;
$dz$ dollar body 4885 ; semicolon inside $dz$
INSERT INTO bench_t_22 (id, payload) VALUES (4886, 'v4886');
SELECT [bracket_4887] FROM [dbo].[tbl_7];
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
BEGIN; SELECT 4889; COMMIT;
$dz$ dollar body 4890 ; semicolon inside $dz$
# hash comment 4891
INSERT INTO bench_t_28 (id, payload) VALUES (4892, 'v4892');
# hash comment 4893
# hash comment 4894
SELECT 4895 AS id, 'row_4895' AS label;
WITH cte_4896 AS (SELECT 4896 AS n) SELECT n FROM cte_4896;
$dz$ dollar body 4897 ; semicolon inside $dz$
SELECT nested FROM t WHERE id IN (4898, 4899, 4900);
-- line 4899: deterministic comment
SELECT * FROM "quoted_4900" WHERE col = E'esc\'4900';
SELECT `mysql_4901` FROM `tbl_1`;
BEGIN; SELECT 4902; COMMIT;
SELECT 4903 AS id, 'row_4903' AS label;
SELECT 4904 AS id, 'row_4904' AS label;
# hash comment 4905
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
$dz$ dollar body 4907 ; semicolon inside $dz$
DELETE FROM bench_t_12 WHERE id = 12;
SELECT nested FROM t WHERE id IN (4909, 4910, 4911);
INSERT INTO bench_t_46 (id, payload) VALUES (4910, 'v4910');
BEGIN; SELECT 4911; COMMIT;
SELECT `mysql_4912` FROM `tbl_12`;
# hash comment 4913
INSERT INTO bench_t_50 (id, payload) VALUES (4914, 'v4914');
SELECT [bracket_4915] FROM [dbo].[tbl_35];
-- line 4916: deterministic comment
WITH cte_4917 AS (SELECT 4917 AS n) SELECT n FROM cte_4917;
# hash comment 4918
BEGIN; SELECT 4919; COMMIT;
/* block header 4920 */
SELECT nested FROM t WHERE id IN (4921, 4922, 4923);
/* block header 4922 */
SELECT 4923 AS id, 'row_4923' AS label;
INSERT INTO bench_t_60 (id, payload) VALUES (4924, 'v4924');
-- line 4925: deterministic comment
SELECT `mysql_4926` FROM `tbl_26`;
DELETE FROM bench_t_31 WHERE id = 15;
DELETE FROM bench_t_0 WHERE id = 0;
INSERT INTO bench_t_65 (id, payload) VALUES (4929, 'v4929');
SELECT `mysql_4930` FROM `tbl_30`;
$dz$ dollar body 4931 ; semicolon inside $dz$
$dz$ dollar body 4932 ; semicolon inside $dz$
SELECT nested FROM t WHERE id IN (4933, 4934, 4935);
UPDATE bench_t_6 SET payload = 4934 WHERE id = 6;
SELECT 4935 AS id, 'row_4935' AS label;
INSERT INTO bench_t_72 (id, payload) VALUES (4936, 'v4936');
SELECT nested FROM t WHERE id IN (4937, 4938, 4939);
SELECT nested FROM t WHERE id IN (4938, 4939, 4940);
BEGIN; SELECT 4939; COMMIT;
WITH cte_4940 AS (SELECT 4940 AS n) SELECT n FROM cte_4940;
$dz$ dollar body 4941 ; semicolon inside $dz$
SELECT nested FROM t WHERE id IN (4942, 4943, 4944);
SELECT [bracket_4943] FROM [dbo].[tbl_23];
UPDATE bench_t_16 SET payload = 4944 WHERE id = 16;
DELETE FROM bench_t_17 WHERE id = 1;
WITH cte_4946 AS (SELECT 4946 AS n) SELECT n FROM cte_4946;
SELECT * FROM "quoted_4947" WHERE col = E'esc\'4947';
BEGIN; SELECT 4948; COMMIT;
BEGIN; SELECT 4949; COMMIT;
SELECT nested FROM t WHERE id IN (4950, 4951, 4952);
SELECT `mysql_4951` FROM `tbl_1`;
WITH cte_4952 AS (SELECT 4952 AS n) SELECT n FROM cte_4952;
UPDATE bench_t_25 SET payload = 4953 WHERE id = 25;
SELECT 4954 AS id, 'row_4954' AS label;
UPDATE bench_t_27 SET payload = 4955 WHERE id = 27;
/* block header 4956 */
# hash comment 4957
SELECT 4958 AS id, 'row_4958' AS label;
/* block header 4959 */
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT nested FROM t WHERE id IN (4961, 4962, 4963);
SELECT [bracket_4962] FROM [dbo].[tbl_2];
BEGIN; SELECT 4963; COMMIT;
SELECT nested FROM t WHERE id IN (4964, 4965, 4966);
SELECT nested FROM t WHERE id IN (4965, 4966, 4967);
SELECT * FROM "quoted_4966" WHERE col = E'esc\'4966';
BEGIN; SELECT 4967; COMMIT;
# hash comment 4968
DELETE FROM bench_t_9 WHERE id = 9;
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
DELETE FROM bench_t_11 WHERE id = 11;
INSERT INTO bench_t_108 (id, payload) VALUES (4972, 'O''Brien');
WITH cte_4973 AS (SELECT 4973 AS n) SELECT n FROM cte_4973;
SELECT `mysql_4974` FROM `tbl_24`;
BEGIN; SELECT 4975; COMMIT;
# hash comment 4976
INSERT INTO bench_t_113 (id, payload) VALUES (4977, 'v4977');
SELECT nested FROM t WHERE id IN (4978, 4979, 4980);
SELECT * FROM "quoted_4979" WHERE col = E'esc\'4979';
/* block header 4980 */
-- line 4981: deterministic comment
INSERT INTO bench_t_118 (id, payload) VALUES (4982, 'v4982');
# hash comment 4983
SELECT `mysql_4984` FROM `tbl_34`;
SELECT `mysql_4985` FROM `tbl_35`;
WITH cte_4986 AS (SELECT 4986 AS n) SELECT n FROM cte_4986;
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
# hash comment 4988
BEGIN; SELECT 4989; COMMIT;
SELECT * FROM "quoted_4990" WHERE col = E'esc\'4990';
-- line 4991: deterministic comment
$dz$ dollar body 4992 ; semicolon inside $dz$
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
WITH cte_4994 AS (SELECT 4994 AS n) SELECT n FROM cte_4994;
-- line 4995: deterministic comment
SELECT * FROM "quoted_4996" WHERE col = E'esc\'4996';
/* block header 4997 */
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
-- line 4999: deterministic comment
/*
 * section 20
 * checksum f40a
 */
SELECT nested FROM t WHERE id IN (5000, 5001, 5002);
-- line 5005: deterministic comment
SELECT [bracket_5006] FROM [dbo].[tbl_6];
DELETE FROM bench_t_15 WHERE id = 15;
UPDATE bench_t_16 SET payload = 5008 WHERE id = 16;
WITH cte_5009 AS (SELECT 5009 AS n) SELECT n FROM cte_5009;
/* block header 5010 */
WITH cte_5011 AS (SELECT 5011 AS n) SELECT n FROM cte_5011;
# hash comment 5012
BEGIN; SELECT 5013; COMMIT;
UPDATE bench_t_22 SET payload = 5014 WHERE id = 22;
$dz$ dollar body 5015 ; semicolon inside $dz$
DELETE FROM bench_t_24 WHERE id = 8;
SELECT `mysql_5017` FROM `tbl_17`;
/* block header 5018 */
WITH cte_5019 AS (SELECT 5019 AS n) SELECT n FROM cte_5019;
-- line 5020: deterministic comment
SELECT * FROM "quoted_5021" WHERE col = E'esc\'5021';
DELETE FROM bench_t_30 WHERE id = 14;
DELETE FROM bench_t_31 WHERE id = 15;
DELETE FROM bench_t_0 WHERE id = 0;
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
UPDATE bench_t_34 SET payload = 5026 WHERE id = 2;
INSERT INTO bench_t_35 (id, payload) VALUES (5027, 'O''Brien');
SELECT 5028 AS id, 'row_5028' AS label;
UPDATE bench_t_37 SET payload = 5029 WHERE id = 5;
SELECT nested FROM t WHERE id IN (5030, 5031, 5032);
UPDATE bench_t_39 SET payload = 5031 WHERE id = 7;
DELETE FROM bench_t_8 WHERE id = 8;
WITH cte_5033 AS (SELECT 5033 AS n) SELECT n FROM cte_5033;
# hash comment 5034
DELETE FROM bench_t_11 WHERE id = 11;
INSERT INTO bench_t_44 (id, payload) VALUES (5036, 'v5036');
SELECT `mysql_5037` FROM `tbl_37`;
SELECT 5038 AS id, 'row_5038' AS label;
SELECT [bracket_5039] FROM [dbo].[tbl_39];
WITH cte_5040 AS (SELECT 5040 AS n) SELECT n FROM cte_5040;
# hash comment 5041
UPDATE bench_t_50 SET payload = 5042 WHERE id = 18;
UPDATE bench_t_51 SET payload = 5043 WHERE id = 19;
INSERT INTO bench_t_52 (id, payload) VALUES (5044, 'v5044');
SELECT nested FROM t WHERE id IN (5045, 5046, 5047);
BEGIN; SELECT 5046; COMMIT;
SELECT nested FROM t WHERE id IN (5047, 5048, 5049);
INSERT INTO bench_t_56 (id, payload) VALUES (5048, 'v5048');
DELETE FROM bench_t_25 WHERE id = 9;
/* block header 5050 */
SELECT [bracket_5051] FROM [dbo].[tbl_11];
INSERT INTO bench_t_60 (id, payload) VALUES (5052, 'v5052');
-- line 5053: deterministic comment
SELECT [bracket_5054] FROM [dbo].[tbl_14];
INSERT INTO bench_t_63 (id, payload) VALUES (5055, 'v5055');
/* block header 5056 */
$dz$ dollar body 5057 ; semicolon inside $dz$
INSERT INTO bench_t_66 (id, payload) VALUES (5058, 'v5058');
DELETE FROM bench_t_3 WHERE id = 3;
SELECT 5060 AS id, 'row_5060' AS label;
SELECT * FROM "quoted_5061" WHERE col = E'esc\'5061';
# hash comment 5062
UPDATE bench_t_7 SET payload = 5063 WHERE id = 7;
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
$dz$ dollar body 5065 ; semicolon inside $dz$
SELECT `mysql_5066` FROM `tbl_16`;
DELETE FROM bench_t_11 WHERE id = 11;
BEGIN; SELECT 5068; COMMIT;
UPDATE bench_t_13 SET payload = 5069 WHERE id = 13;
BEGIN; SELECT 5070; COMMIT;
WITH cte_5071 AS (SELECT 5071 AS n) SELECT n FROM cte_5071;
SELECT * FROM "quoted_5072" WHERE col = E'esc\'5072';
SELECT * FROM "quoted_5073" WHERE col = E'esc\'5073';
UPDATE bench_t_18 SET payload = 5074 WHERE id = 18;
SELECT * FROM "quoted_5075" WHERE col = E'esc\'5075';
SELECT nested FROM t WHERE id IN (5076, 5077, 5078);
SELECT * FROM "quoted_5077" WHERE col = E'esc\'5077';
SELECT `mysql_5078` FROM `tbl_28`;
WITH cte_5079 AS (SELECT 5079 AS n) SELECT n FROM cte_5079;
SELECT * FROM "quoted_5080" WHERE col = E'esc\'5080';
UPDATE bench_t_25 SET payload = 5081 WHERE id = 25;
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT `mysql_5083` FROM `tbl_33`;
SELECT [bracket_5084] FROM [dbo].[tbl_4];
DELETE FROM bench_t_29 WHERE id = 13;
SELECT [bracket_5086] FROM [dbo].[tbl_6];
DELETE FROM bench_t_31 WHERE id = 15;
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
UPDATE bench_t_33 SET payload = 5089 WHERE id = 1;
-- line 5090: deterministic comment
SELECT `mysql_5091` FROM `tbl_41`;
/* block header 5092 */
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
INSERT INTO bench_t_102 (id, payload) VALUES (5094, 'v5094');
INSERT INTO bench_t_103 (id, payload) VALUES (5095, 'v5095');
SELECT * FROM "quoted_5096" WHERE col = E'esc\'5096';
SELECT nested FROM t WHERE id IN (5097, 5098, 5099);
DELETE FROM bench_t_10 WHERE id = 10;
SELECT nested FROM t WHERE id IN (5099, 5100, 5101);
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT [bracket_5101] FROM [dbo].[tbl_21];
-- line 5102: deterministic comment
SELECT 5103 AS id, 'row_5103' AS label;
SELECT * FROM "quoted_5104" WHERE col = E'esc\'5104';
$dz$ dollar body 5105 ; semicolon inside $dz$
-- line 5106: deterministic comment
$dz$ dollar body 5107 ; semicolon inside $dz$
# hash comment 5108
SELECT nested FROM t WHERE id IN (5109, 5110, 5111);
SELECT 5110 AS id, 'row_5110' AS label;
INSERT INTO bench_t_119 (id, payload) VALUES (5111, 'v5111');
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT nested FROM t WHERE id IN (5113, 5114, 5115);
UPDATE bench_t_58 SET payload = 5114 WHERE id = 26;
SELECT nested FROM t WHERE id IN (5115, 5116, 5117);
SELECT [bracket_5116] FROM [dbo].[tbl_36];
SELECT [bracket_5117] FROM [dbo].[tbl_37];
/* block header 5118 */
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
-- line 5120: deterministic comment
UPDATE bench_t_1 SET payload = 5121 WHERE id = 1;
SELECT [bracket_5122] FROM [dbo].[tbl_2];
$dz$ dollar body 5123 ; semicolon inside $dz$
UPDATE bench_t_4 SET payload = 5124 WHERE id = 4;
BEGIN; SELECT 5125; COMMIT;
$dz$ dollar body 5126 ; semicolon inside $dz$
SELECT [bracket_5127] FROM [dbo].[tbl_7];
INSERT INTO bench_t_8 (id, payload) VALUES (5128, 'v5128');
$dz$ dollar body 5129 ; semicolon inside $dz$
INSERT INTO bench_t_10 (id, payload) VALUES (5130, 'v5130');
DELETE FROM bench_t_11 WHERE id = 11;
SELECT * FROM "quoted_5132" WHERE col = E'esc\'5132';
SELECT nested FROM t WHERE id IN (5133, 5134, 5135);
SELECT [bracket_5134] FROM [dbo].[tbl_14];
UPDATE bench_t_15 SET payload = 5135 WHERE id = 15;
$dz$ dollar body 5136 ; semicolon inside $dz$
-- line 5137: deterministic comment
SELECT 5138 AS id, 'row_5138' AS label;
BEGIN; SELECT 5139; COMMIT;
SELECT `mysql_5140` FROM `tbl_40`;
DELETE FROM bench_t_21 WHERE id = 5;
/* block header 5142 */
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
WITH cte_5144 AS (SELECT 5144 AS n) SELECT n FROM cte_5144;
WITH cte_5145 AS (SELECT 5145 AS n) SELECT n FROM cte_5145;
$dz$ dollar body 5146 ; semicolon inside $dz$
INSERT INTO bench_t_27 (id, payload) VALUES (5147, 'v5147');
SELECT [bracket_5148] FROM [dbo].[tbl_28];
BEGIN; SELECT 5149; COMMIT;
# hash comment 5150
BEGIN; SELECT 5151; COMMIT;
# hash comment 5152
WITH cte_5153 AS (SELECT 5153 AS n) SELECT n FROM cte_5153;
# hash comment 5154
DELETE FROM bench_t_3 WHERE id = 3;
-- line 5156: deterministic comment
BEGIN; SELECT 5157; COMMIT;
$dz$ dollar body 5158 ; semicolon inside $dz$
$dz$ dollar body 5159 ; semicolon inside $dz$
# hash comment 5160
/* block header 5161 */
SELECT * FROM "quoted_5162" WHERE col = E'esc\'5162';
# hash comment 5163
$dz$ dollar body 5164 ; semicolon inside $dz$
# hash comment 5165
SELECT [bracket_5166] FROM [dbo].[tbl_6];
SELECT 5167 AS id, 'row_5167' AS label;
SELECT [bracket_5168] FROM [dbo].[tbl_8];
$dz$ dollar body 5169 ; semicolon inside $dz$
INSERT INTO bench_t_50 (id, payload) VALUES (5170, 'O''Brien');
BEGIN; SELECT 5171; COMMIT;
SELECT 5172 AS id, 'row_5172' AS label;
$dz$ dollar body 5173 ; semicolon inside $dz$
SELECT 5174 AS id, 'row_5174' AS label;
DELETE FROM bench_t_23 WHERE id = 7;
SELECT `mysql_5176` FROM `tbl_26`;
SELECT 5177 AS id, 'row_5177' AS label;
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT 5179 AS id, 'row_5179' AS label;
SELECT 5180 AS id, 'row_5180' AS label;
-- line 5181: deterministic comment
$dz$ dollar body 5182 ; semicolon inside $dz$
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT `mysql_5184` FROM `tbl_34`;
BEGIN; SELECT 5185; COMMIT;
WITH cte_5186 AS (SELECT 5186 AS n) SELECT n FROM cte_5186;
# hash comment 5187
SELECT `mysql_5188` FROM `tbl_38`;
# hash comment 5189
/* block header 5190 */
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT [bracket_5192] FROM [dbo].[tbl_32];
SELECT [bracket_5193] FROM [dbo].[tbl_33];
DELETE FROM bench_t_10 WHERE id = 10;
SELECT nested FROM t WHERE id IN (5195, 5196, 5197);
SELECT 5196 AS id, 'row_5196' AS label;
UPDATE bench_t_13 SET payload = 5197 WHERE id = 13;
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
# hash comment 5200
# hash comment 5201
SELECT nested FROM t WHERE id IN (5202, 5203, 5204);
SELECT nested FROM t WHERE id IN (5203, 5204, 5205);
SELECT 5204 AS id, 'row_5204' AS label;
SELECT 5205 AS id, 'row_5205' AS label;
$dz$ dollar body 5206 ; semicolon inside $dz$
# hash comment 5207
/* block header 5208 */
SELECT * FROM "quoted_5209" WHERE col = E'esc\'5209';
UPDATE bench_t_26 SET payload = 5210 WHERE id = 26;
INSERT INTO bench_t_91 (id, payload) VALUES (5211, 'v5211');
WITH cte_5212 AS (SELECT 5212 AS n) SELECT n FROM cte_5212;
SELECT `mysql_5213` FROM `tbl_13`;
UPDATE bench_t_30 SET payload = 5214 WHERE id = 30;
UPDATE bench_t_31 SET payload = 5215 WHERE id = 31;
SELECT `mysql_5216` FROM `tbl_16`;
-- line 5217: deterministic comment
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
INSERT INTO bench_t_100 (id, payload) VALUES (5220, 'v5220');
SELECT 5221 AS id, 'row_5221' AS label;
-- line 5222: deterministic comment
# hash comment 5223
SELECT [bracket_5224] FROM [dbo].[tbl_24];
-- line 5225: deterministic comment
/* block header 5226 */
# hash comment 5227
WITH cte_5228 AS (SELECT 5228 AS n) SELECT n FROM cte_5228;
UPDATE bench_t_45 SET payload = 5229 WHERE id = 13;
SELECT * FROM "quoted_5230" WHERE col = E'esc\'5230';
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
UPDATE bench_t_48 SET payload = 5232 WHERE id = 16;
$dz$ dollar body 5233 ; semicolon inside $dz$
SELECT `mysql_5234` FROM `tbl_34`;
SELECT [bracket_5235] FROM [dbo].[tbl_35];
SELECT [bracket_5236] FROM [dbo].[tbl_36];
BEGIN; SELECT 5237; COMMIT;
SELECT * FROM "quoted_5238" WHERE col = E'esc\'5238';
# hash comment 5239
SELECT `mysql_5240` FROM `tbl_40`;
/* block header 5241 */
/* block header 5242 */
INSERT INTO bench_t_123 (id, payload) VALUES (5243, 'v5243');
SELECT nested FROM t WHERE id IN (5244, 5245, 5246);
INSERT INTO bench_t_125 (id, payload) VALUES (5245, 'v5245');
/* block header 5246 */
SELECT nested FROM t WHERE id IN (5247, 5248, 5249);
$dz$ dollar body 5248 ; semicolon inside $dz$
UPDATE bench_t_1 SET payload = 5249 WHERE id = 1;
/*
 * section 21
 * checksum 3d52
 */
$dz$ dollar body 5250 ; semicolon inside $dz$
UPDATE bench_t_7 SET payload = 5255 WHERE id = 7;
BEGIN; SELECT 5256; COMMIT;
SELECT * FROM "quoted_5257" WHERE col = E'esc\'5257';
WITH cte_5258 AS (SELECT 5258 AS n) SELECT n FROM cte_5258;
# hash comment 5259
$dz$ dollar body 5260 ; semicolon inside $dz$
/* block header 5261 */
INSERT INTO bench_t_14 (id, payload) VALUES (5262, 'v5262');
SELECT 5263 AS id, 'row_5263' AS label;
INSERT INTO bench_t_16 (id, payload) VALUES (5264, 'v5264');
SELECT `mysql_5265` FROM `tbl_15`;
-- line 5266: deterministic comment
SELECT 5267 AS id, 'row_5267' AS label;
SELECT * FROM "quoted_5268" WHERE col = E'esc\'5268';
UPDATE bench_t_21 SET payload = 5269 WHERE id = 21;
UPDATE bench_t_22 SET payload = 5270 WHERE id = 22;
SELECT 5271 AS id, 'row_5271' AS label;
/* block header 5272 */
SELECT * FROM "quoted_5273" WHERE col = E'esc\'5273';
SELECT nested FROM t WHERE id IN (5274, 5275, 5276);
BEGIN; SELECT 5275; COMMIT;
SELECT * FROM "quoted_5276" WHERE col = E'esc\'5276';
$dz$ dollar body 5277 ; semicolon inside $dz$
-- line 5278: deterministic comment
/* block header 5279 */
SELECT * FROM "quoted_5280" WHERE col = E'esc\'5280';
UPDATE bench_t_33 SET payload = 5281 WHERE id = 1;
SELECT `mysql_5282` FROM `tbl_32`;
-- line 5283: deterministic comment
SELECT [bracket_5284] FROM [dbo].[tbl_4];
# hash comment 5285
-- line 5286: deterministic comment
DELETE FROM bench_t_7 WHERE id = 7;
SELECT * FROM "quoted_5288" WHERE col = E'esc\'5288';
SELECT [bracket_5289] FROM [dbo].[tbl_9];
/* block header 5290 */
SELECT nested FROM t WHERE id IN (5291, 5292, 5293);
DELETE FROM bench_t_12 WHERE id = 12;
SELECT 5293 AS id, 'row_5293' AS label;
WITH cte_5294 AS (SELECT 5294 AS n) SELECT n FROM cte_5294;
SELECT 5295 AS id, 'row_5295' AS label;
SELECT * FROM "quoted_5296" WHERE col = E'esc\'5296';
SELECT [bracket_5297] FROM [dbo].[tbl_17];
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
BEGIN; SELECT 5300; COMMIT;
SELECT * FROM "quoted_5301" WHERE col = E'esc\'5301';
BEGIN; SELECT 5302; COMMIT;
# hash comment 5303
INSERT INTO bench_t_56 (id, payload) VALUES (5304, 'v5304');
WITH cte_5305 AS (SELECT 5305 AS n) SELECT n FROM cte_5305;
DELETE FROM bench_t_26 WHERE id = 10;
-- line 5307: deterministic comment
$dz$ dollar body 5308 ; semicolon inside $dz$
SELECT `mysql_5309` FROM `tbl_9`;
SELECT `mysql_5310` FROM `tbl_10`;
SELECT 5311 AS id, 'row_5311' AS label;
SELECT * FROM "quoted_5312" WHERE col = E'esc\'5312';
INSERT INTO bench_t_65 (id, payload) VALUES (5313, 'O''Brien');
SELECT 5314 AS id, 'row_5314' AS label;
# hash comment 5315
UPDATE bench_t_4 SET payload = 5316 WHERE id = 4;
WITH cte_5317 AS (SELECT 5317 AS n) SELECT n FROM cte_5317;
$dz$ dollar body 5318 ; semicolon inside $dz$
SELECT * FROM "quoted_5319" WHERE col = E'esc\'5319';
UPDATE bench_t_8 SET payload = 5320 WHERE id = 8;
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT [bracket_5322] FROM [dbo].[tbl_2];
DELETE FROM bench_t_11 WHERE id = 11;
/* block header 5324 */
SELECT 5325 AS id, 'row_5325' AS label;
BEGIN; SELECT 5326; COMMIT;
SELECT [bracket_5327] FROM [dbo].[tbl_7];
$dz$ dollar body 5328 ; semicolon inside $dz$
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT `mysql_5330` FROM `tbl_30`;
BEGIN; SELECT 5331; COMMIT;
SELECT [bracket_5332] FROM [dbo].[tbl_12];
-- line 5333: deterministic comment
/* block header 5334 */
DELETE FROM bench_t_23 WHERE id = 7;
SELECT `mysql_5336` FROM `tbl_36`;
SELECT * FROM "quoted_5337" WHERE col = E'esc\'5337';
DELETE FROM bench_t_26 WHERE id = 10;
$dz$ dollar body 5339 ; semicolon inside $dz$
BEGIN; SELECT 5340; COMMIT;
# hash comment 5341
SELECT `mysql_5342` FROM `tbl_42`;
INSERT INTO bench_t_95 (id, payload) VALUES (5343, 'v5343');
WITH cte_5344 AS (SELECT 5344 AS n) SELECT n FROM cte_5344;
SELECT * FROM "quoted_5345" WHERE col = E'esc\'5345';
/* block header 5346 */
# hash comment 5347
-- line 5348: deterministic comment
SELECT `mysql_5349` FROM `tbl_49`;
SELECT [bracket_5350] FROM [dbo].[tbl_30];
/* block header 5351 */
# hash comment 5352
$dz$ dollar body 5353 ; semicolon inside $dz$
UPDATE bench_t_42 SET payload = 5354 WHERE id = 10;
DELETE FROM bench_t_11 WHERE id = 11;
-- line 5356: deterministic comment
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
WITH cte_5358 AS (SELECT 5358 AS n) SELECT n FROM cte_5358;
INSERT INTO bench_t_111 (id, payload) VALUES (5359, 'v5359');
WITH cte_5360 AS (SELECT 5360 AS n) SELECT n FROM cte_5360;
-- line 5361: deterministic comment
BEGIN; SELECT 5362; COMMIT;
SELECT [bracket_5363] FROM [dbo].[tbl_3];
BEGIN; SELECT 5364; COMMIT;
-- line 5365: deterministic comment
BEGIN; SELECT 5366; COMMIT;
# hash comment 5367
BEGIN; SELECT 5368; COMMIT;
BEGIN; SELECT 5369; COMMIT;
BEGIN; SELECT 5370; COMMIT;
WITH cte_5371 AS (SELECT 5371 AS n) SELECT n FROM cte_5371;
SELECT * FROM "quoted_5372" WHERE col = E'esc\'5372';
SELECT 5373 AS id, 'row_5373' AS label;
INSERT INTO bench_t_126 (id, payload) VALUES (5374, 'v5374');
/* block header 5375 */
BEGIN; SELECT 5376; COMMIT;
WITH cte_5377 AS (SELECT 5377 AS n) SELECT n FROM cte_5377;
SELECT `mysql_5378` FROM `tbl_28`;
WITH cte_5379 AS (SELECT 5379 AS n) SELECT n FROM cte_5379;
/* block header 5380 */
DELETE FROM bench_t_5 WHERE id = 5;
UPDATE bench_t_6 SET payload = 5382 WHERE id = 6;
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
INSERT INTO bench_t_8 (id, payload) VALUES (5384, 'v5384');
WITH cte_5385 AS (SELECT 5385 AS n) SELECT n FROM cte_5385;
SELECT [bracket_5386] FROM [dbo].[tbl_26];
SELECT nested FROM t WHERE id IN (5387, 5388, 5389);
# hash comment 5388
-- line 5389: deterministic comment
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
-- line 5391: deterministic comment
WITH cte_5392 AS (SELECT 5392 AS n) SELECT n FROM cte_5392;
SELECT nested FROM t WHERE id IN (5393, 5394, 5395);
SELECT `mysql_5394` FROM `tbl_44`;
BEGIN; SELECT 5395; COMMIT;
UPDATE bench_t_20 SET payload = 5396 WHERE id = 20;
INSERT INTO bench_t_21 (id, payload) VALUES (5397, 'v5397');
-- line 5398: deterministic comment
SELECT nested FROM t WHERE id IN (5399, 5400, 5401);
UPDATE bench_t_24 SET payload = 5400 WHERE id = 24;
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
BEGIN; SELECT 5402; COMMIT;
# hash comment 5403
DELETE FROM bench_t_28 WHERE id = 12;
/* block header 5405 */
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT `mysql_5407` FROM `tbl_7`;
/* block header 5408 */
SELECT [bracket_5409] FROM [dbo].[tbl_9];
/* block header 5410 */
$dz$ dollar body 5411 ; semicolon inside $dz$
UPDATE bench_t_36 SET payload = 5412 WHERE id = 4;
UPDATE bench_t_37 SET payload = 5413 WHERE id = 5;
SELECT 5414 AS id, 'row_5414' AS label;
# hash comment 5415
/* block header 5416 */
# hash comment 5417
INSERT INTO bench_t_42 (id, payload) VALUES (5418, 'v5418');
$dz$ dollar body 5419 ; semicolon inside $dz$
SELECT * FROM "quoted_5420" WHERE col = E'esc\'5420';
INSERT INTO bench_t_45 (id, payload) VALUES (5421, 'v5421');
SELECT 5422 AS id, 'row_5422' AS label;
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
BEGIN; SELECT 5424; COMMIT;
SELECT * FROM "quoted_5425" WHERE col = E'esc\'5425';
SELECT * FROM "quoted_5426" WHERE col = E'esc\'5426';
SELECT `mysql_5427` FROM `tbl_27`;
INSERT INTO bench_t_52 (id, payload) VALUES (5428, 'v5428');
SELECT * FROM "quoted_5429" WHERE col = E'esc\'5429';
WITH cte_5430 AS (SELECT 5430 AS n) SELECT n FROM cte_5430;
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT nested FROM t WHERE id IN (5432, 5433, 5434);
SELECT * FROM "quoted_5433" WHERE col = E'esc\'5433';
BEGIN; SELECT 5434; COMMIT;
DELETE FROM bench_t_27 WHERE id = 11;
$dz$ dollar body 5436 ; semicolon inside $dz$
UPDATE bench_t_61 SET payload = 5437 WHERE id = 29;
UPDATE bench_t_62 SET payload = 5438 WHERE id = 30;
# hash comment 5439
SELECT * FROM "quoted_5440" WHERE col = E'esc\'5440';
DELETE FROM bench_t_1 WHERE id = 1;
SELECT `mysql_5442` FROM `tbl_42`;
BEGIN; SELECT 5443; COMMIT;
SELECT `mysql_5444` FROM `tbl_44`;
DELETE FROM bench_t_5 WHERE id = 5;
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT [bracket_5447] FROM [dbo].[tbl_7];
INSERT INTO bench_t_72 (id, payload) VALUES (5448, 'v5448');
SELECT nested FROM t WHERE id IN (5449, 5450, 5451);
INSERT INTO bench_t_74 (id, payload) VALUES (5450, 'v5450');
/* block header 5451 */
SELECT [bracket_5452] FROM [dbo].[tbl_12];
-- line 5453: deterministic comment
WITH cte_5454 AS (SELECT 5454 AS n) SELECT n FROM cte_5454;
DELETE FROM bench_t_15 WHERE id = 15;
BEGIN; SELECT 5456; COMMIT;
SELECT * FROM "quoted_5457" WHERE col = E'esc\'5457';
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
-- line 5459: deterministic comment
SELECT nested FROM t WHERE id IN (5460, 5461, 5462);
INSERT INTO bench_t_85 (id, payload) VALUES (5461, 'v5461');
$dz$ dollar body 5462 ; semicolon inside $dz$
-- line 5463: deterministic comment
# hash comment 5464
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT * FROM "quoted_5466" WHERE col = E'esc\'5466';
SELECT * FROM "quoted_5467" WHERE col = E'esc\'5467';
DELETE FROM bench_t_28 WHERE id = 12;
INSERT INTO bench_t_93 (id, payload) VALUES (5469, 'v5469');
SELECT 5470 AS id, 'row_5470' AS label;
# hash comment 5471
/* block header 5472 */
SELECT nested FROM t WHERE id IN (5473, 5474, 5475);
DELETE FROM bench_t_2 WHERE id = 2;
SELECT * FROM "quoted_5475" WHERE col = E'esc\'5475';
UPDATE bench_t_36 SET payload = 5476 WHERE id = 4;
INSERT INTO bench_t_101 (id, payload) VALUES (5477, 'v5477');
WITH cte_5478 AS (SELECT 5478 AS n) SELECT n FROM cte_5478;
WITH cte_5479 AS (SELECT 5479 AS n) SELECT n FROM cte_5479;
$dz$ dollar body 5480 ; semicolon inside $dz$
SELECT 5481 AS id, 'row_5481' AS label;
/* block header 5482 */
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
BEGIN; SELECT 5484; COMMIT;
SELECT `mysql_5485` FROM `tbl_35`;
$dz$ dollar body 5486 ; semicolon inside $dz$
$dz$ dollar body 5487 ; semicolon inside $dz$
SELECT * FROM "quoted_5488" WHERE col = E'esc\'5488';
UPDATE bench_t_49 SET payload = 5489 WHERE id = 17;
SELECT [bracket_5490] FROM [dbo].[tbl_10];
SELECT [bracket_5491] FROM [dbo].[tbl_11];
# hash comment 5492
SELECT 5493 AS id, 'row_5493' AS label;
UPDATE bench_t_54 SET payload = 5494 WHERE id = 22;
-- line 5495: deterministic comment
BEGIN; SELECT 5496; COMMIT;
SELECT [bracket_5497] FROM [dbo].[tbl_17];
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
-- line 5499: deterministic comment
/*
 * section 22
 * checksum 10c
 */
SELECT `mysql_5500` FROM `tbl_0`;
SELECT `mysql_5505` FROM `tbl_5`;
-- line 5506: deterministic comment
# hash comment 5507
$dz$ dollar body 5508 ; semicolon inside $dz$
# hash comment 5509
# hash comment 5510
SELECT nested FROM t WHERE id IN (5511, 5512, 5513);
WITH cte_5512 AS (SELECT 5512 AS n) SELECT n FROM cte_5512;
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT * FROM "quoted_5514" WHERE col = E'esc\'5514';
/* block header 5515 */
SELECT [bracket_5516] FROM [dbo].[tbl_36];
SELECT `mysql_5517` FROM `tbl_17`;
SELECT * FROM "quoted_5518" WHERE col = E'esc\'5518';
DELETE FROM bench_t_15 WHERE id = 15;
UPDATE bench_t_16 SET payload = 5520 WHERE id = 16;
-- line 5521: deterministic comment
SELECT * FROM "quoted_5522" WHERE col = E'esc\'5522';
/* block header 5523 */
/* block header 5524 */
UPDATE bench_t_21 SET payload = 5525 WHERE id = 21;
SELECT * FROM "quoted_5526" WHERE col = E'esc\'5526';
BEGIN; SELECT 5527; COMMIT;
SELECT `mysql_5528` FROM `tbl_28`;
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
DELETE FROM bench_t_26 WHERE id = 10;
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT nested FROM t WHERE id IN (5532, 5533, 5534);
-- line 5533: deterministic comment
WITH cte_5534 AS (SELECT 5534 AS n) SELECT n FROM cte_5534;
BEGIN; SELECT 5535; COMMIT;
SELECT nested FROM t WHERE id IN (5536, 5537, 5538);
-- line 5537: deterministic comment
INSERT INTO bench_t_34 (id, payload) VALUES (5538, 'v5538');
INSERT INTO bench_t_35 (id, payload) VALUES (5539, 'v5539');
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
INSERT INTO bench_t_37 (id, payload) VALUES (5541, 'v5541');
SELECT * FROM "quoted_5542" WHERE col = E'esc\'5542';
SELECT [bracket_5543] FROM [dbo].[tbl_23];
DELETE FROM bench_t_8 WHERE id = 8;
SELECT [bracket_5545] FROM [dbo].[tbl_25];
SELECT * FROM "quoted_5546" WHERE col = E'esc\'5546';
$dz$ dollar body 5547 ; semicolon inside $dz$
SELECT [bracket_5548] FROM [dbo].[tbl_28];
SELECT [bracket_5549] FROM [dbo].[tbl_29];
BEGIN; SELECT 5550; COMMIT;
SELECT * FROM "quoted_5551" WHERE col = E'esc\'5551';
SELECT 5552 AS id, 'row_5552' AS label;
/* block header 5553 */
# hash comment 5554
UPDATE bench_t_51 SET payload = 5555 WHERE id = 19;
$dz$ dollar body 5556 ; semicolon inside $dz$
SELECT nested FROM t WHERE id IN (5557, 5558, 5559);
SELECT 5558 AS id, 'row_5558' AS label;
# hash comment 5559
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT `mysql_5561` FROM `tbl_11`;
UPDATE bench_t_58 SET payload = 5562 WHERE id = 26;
SELECT * FROM "quoted_5563" WHERE col = E'esc\'5563';
-- line 5564: deterministic comment
INSERT INTO bench_t_61 (id, payload) VALUES (5565, 'v5565');
BEGIN; SELECT 5566; COMMIT;
$dz$ dollar body 5567 ; semicolon inside $dz$
SELECT nested FROM t WHERE id IN (5568, 5569, 5570);
INSERT INTO bench_t_65 (id, payload) VALUES (5569, 'v5569');
BEGIN; SELECT 5570; COMMIT;
UPDATE bench_t_3 SET payload = 5571 WHERE id = 3;
BEGIN; SELECT 5572; COMMIT;
UPDATE bench_t_5 SET payload = 5573 WHERE id = 5;
$dz$ dollar body 5574 ; semicolon inside $dz$
DELETE FROM bench_t_7 WHERE id = 7;
WITH cte_5576 AS (SELECT 5576 AS n) SELECT n FROM cte_5576;
SELECT 5577 AS id, 'row_5577' AS label;
BEGIN; SELECT 5578; COMMIT;
/* block header 5579 */
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT * FROM "quoted_5581" WHERE col = E'esc\'5581';
BEGIN; SELECT 5582; COMMIT;
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
DELETE FROM bench_t_16 WHERE id = 0;
DELETE FROM bench_t_17 WHERE id = 1;
$dz$ dollar body 5586 ; semicolon inside $dz$
/* block header 5587 */
/* block header 5588 */
DELETE FROM bench_t_21 WHERE id = 5;
SELECT * FROM "quoted_5590" WHERE col = E'esc\'5590';
UPDATE bench_t_23 SET payload = 5591 WHERE id = 23;
-- line 5592: deterministic comment
$dz$ dollar body 5593 ; semicolon inside $dz$
$dz$ dollar body 5594 ; semicolon inside $dz$
SELECT nested FROM t WHERE id IN (5595, 5596, 5597);
/* block header 5596 */
SELECT 5597 AS id, 'row_5597' AS label;
-- line 5598: deterministic comment
DELETE FROM bench_t_31 WHERE id = 15;
UPDATE bench_t_32 SET payload = 5600 WHERE id = 0;
SELECT [bracket_5601] FROM [dbo].[tbl_1];
SELECT 5602 AS id, 'row_5602' AS label;
/* block header 5603 */
WITH cte_5604 AS (SELECT 5604 AS n) SELECT n FROM cte_5604;
/* block header 5605 */
SELECT 5606 AS id, 'row_5606' AS label;
$dz$ dollar body 5607 ; semicolon inside $dz$
SELECT 5608 AS id, 'row_5608' AS label;
/* block header 5609 */
# hash comment 5610
# hash comment 5611
WITH cte_5612 AS (SELECT 5612 AS n) SELECT n FROM cte_5612;
-- line 5613: deterministic comment
# hash comment 5614
SELECT nested FROM t WHERE id IN (5615, 5616, 5617);
WITH cte_5616 AS (SELECT 5616 AS n) SELECT n FROM cte_5616;
SELECT nested FROM t WHERE id IN (5617, 5618, 5619);
SELECT nested FROM t WHERE id IN (5618, 5619, 5620);
INSERT INTO bench_t_115 (id, payload) VALUES (5619, 'v5619');
$dz$ dollar body 5620 ; semicolon inside $dz$
UPDATE bench_t_53 SET payload = 5621 WHERE id = 21;
SELECT [bracket_5622] FROM [dbo].[tbl_22];
WITH cte_5623 AS (SELECT 5623 AS n) SELECT n FROM cte_5623;
DELETE FROM bench_t_24 WHERE id = 8;
BEGIN; SELECT 5625; COMMIT;
SELECT `mysql_5626` FROM `tbl_26`;
# hash comment 5627
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
INSERT INTO bench_t_125 (id, payload) VALUES (5629, 'v5629');
# hash comment 5630
SELECT nested FROM t WHERE id IN (5631, 5632, 5633);
SELECT `mysql_5632` FROM `tbl_32`;
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
/* block header 5634 */
INSERT INTO bench_t_3 (id, payload) VALUES (5635, 'v5635');
-- line 5636: deterministic comment
/* block header 5637 */
SELECT nested FROM t WHERE id IN (5638, 5639, 5640);
INSERT INTO bench_t_7 (id, payload) VALUES (5639, 'v5639');
# hash comment 5640
BEGIN; SELECT 5641; COMMIT;
INSERT INTO bench_t_10 (id, payload) VALUES (5642, 'v5642');
DELETE FROM bench_t_11 WHERE id = 11;
/* block header 5644 */
INSERT INTO bench_t_13 (id, payload) VALUES (5645, 'v5645');
SELECT `mysql_5646` FROM `tbl_46`;
DELETE FROM bench_t_15 WHERE id = 15;
SELECT 5648 AS id, 'row_5648' AS label;
UPDATE bench_t_17 SET payload = 5649 WHERE id = 17;
SELECT * FROM "quoted_5650" WHERE col = E'esc\'5650';
INSERT INTO bench_t_19 (id, payload) VALUES (5651, 'v5651');
WITH cte_5652 AS (SELECT 5652 AS n) SELECT n FROM cte_5652;
DELETE FROM bench_t_21 WHERE id = 5;
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
# hash comment 5655
WITH cte_5656 AS (SELECT 5656 AS n) SELECT n FROM cte_5656;
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
# hash comment 5658
SELECT `mysql_5659` FROM `tbl_9`;
SELECT [bracket_5660] FROM [dbo].[tbl_20];
/* block header 5661 */
SELECT 5662 AS id, 'row_5662' AS label;
BEGIN; SELECT 5663; COMMIT;
WITH cte_5664 AS (SELECT 5664 AS n) SELECT n FROM cte_5664;
$dz$ dollar body 5665 ; semicolon inside $dz$
BEGIN; SELECT 5666; COMMIT;
-- line 5667: deterministic comment
WITH cte_5668 AS (SELECT 5668 AS n) SELECT n FROM cte_5668;
BEGIN; SELECT 5669; COMMIT;
INSERT INTO bench_t_38 (id, payload) VALUES (5670, 'v5670');
DELETE FROM bench_t_7 WHERE id = 7;
-- line 5672: deterministic comment
UPDATE bench_t_41 SET payload = 5673 WHERE id = 9;
BEGIN; SELECT 5674; COMMIT;
INSERT INTO bench_t_43 (id, payload) VALUES (5675, 'v5675');
SELECT 5676 AS id, 'row_5676' AS label;
SELECT [bracket_5677] FROM [dbo].[tbl_37];
BEGIN; SELECT 5678; COMMIT;
SELECT [bracket_5679] FROM [dbo].[tbl_39];
SELECT 5680 AS id, 'row_5680' AS label;
/* block header 5681 */
DELETE FROM bench_t_18 WHERE id = 2;
-- line 5683: deterministic comment
WITH cte_5684 AS (SELECT 5684 AS n) SELECT n FROM cte_5684;
# hash comment 5685
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
-- line 5687: deterministic comment
-- line 5688: deterministic comment
# hash comment 5689
# hash comment 5690
SELECT nested FROM t WHERE id IN (5691, 5692, 5693);
SELECT 5692 AS id, 'row_5692' AS label;
/* block header 5693 */
DELETE FROM bench_t_30 WHERE id = 14;
UPDATE bench_t_63 SET payload = 5695 WHERE id = 31;
INSERT INTO bench_t_64 (id, payload) VALUES (5696, 'v5696');
SELECT nested FROM t WHERE id IN (5697, 5698, 5699);
/* block header 5698 */
UPDATE bench_t_3 SET payload = 5699 WHERE id = 3;
SELECT nested FROM t WHERE id IN (5700, 5701, 5702);
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT [bracket_5702] FROM [dbo].[tbl_22];
SELECT nested FROM t WHERE id IN (5703, 5704, 5705);
# hash comment 5704
SELECT [bracket_5705] FROM [dbo].[tbl_25];
SELECT * FROM "quoted_5706" WHERE col = E'esc\'5706';
SELECT `mysql_5707` FROM `tbl_7`;
SELECT `mysql_5708` FROM `tbl_8`;
SELECT `mysql_5709` FROM `tbl_9`;
# hash comment 5710
-- line 5711: deterministic comment
SELECT [bracket_5712] FROM [dbo].[tbl_32];
BEGIN; SELECT 5713; COMMIT;
$dz$ dollar body 5714 ; semicolon inside $dz$
# hash comment 5715
SELECT * FROM "quoted_5716" WHERE col = E'esc\'5716';
SELECT 5717 AS id, 'row_5717' AS label;
DELETE FROM bench_t_22 WHERE id = 6;
BEGIN; SELECT 5719; COMMIT;
BEGIN; SELECT 5720; COMMIT;
WITH cte_5721 AS (SELECT 5721 AS n) SELECT n FROM cte_5721;
DELETE FROM bench_t_26 WHERE id = 10;
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
DELETE FROM bench_t_28 WHERE id = 12;
INSERT INTO bench_t_93 (id, payload) VALUES (5725, 'v5725');
SELECT nested FROM t WHERE id IN (5726, 5727, 5728);
SELECT `mysql_5727` FROM `tbl_27`;
# hash comment 5728
$dz$ dollar body 5729 ; semicolon inside $dz$
$dz$ dollar body 5730 ; semicolon inside $dz$
BEGIN; SELECT 5731; COMMIT;
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
$dz$ dollar body 5734 ; semicolon inside $dz$
SELECT nested FROM t WHERE id IN (5735, 5736, 5737);
SELECT 5736 AS id, 'row_5736' AS label;
SELECT nested FROM t WHERE id IN (5737, 5738, 5739);
UPDATE bench_t_42 SET payload = 5738 WHERE id = 10;
BEGIN; SELECT 5739; COMMIT;
$dz$ dollar body 5740 ; semicolon inside $dz$
DELETE FROM bench_t_13 WHERE id = 13;
SELECT `mysql_5742` FROM `tbl_42`;
BEGIN; SELECT 5743; COMMIT;
SELECT [bracket_5744] FROM [dbo].[tbl_24];
SELECT [bracket_5745] FROM [dbo].[tbl_25];
/* block header 5746 */
# hash comment 5747
SELECT `mysql_5748` FROM `tbl_48`;
SELECT nested FROM t WHERE id IN (5749, 5750, 5751);
/*
 * section 23
 * checksum 1b96
 */
WITH cte_5750 AS (SELECT 5750 AS n) SELECT n FROM cte_5750;
UPDATE bench_t_59 SET payload = 5755 WHERE id = 27;
DELETE FROM bench_t_28 WHERE id = 12;
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
$dz$ dollar body 5758 ; semicolon inside $dz$
SELECT * FROM "quoted_5759" WHERE col = E'esc\'5759';
SELECT * FROM "quoted_5760" WHERE col = E'esc\'5760';
WITH cte_5761 AS (SELECT 5761 AS n) SELECT n FROM cte_5761;
/* block header 5762 */
WITH cte_5763 AS (SELECT 5763 AS n) SELECT n FROM cte_5763;
$dz$ dollar body 5764 ; semicolon inside $dz$
SELECT 5765 AS id, 'row_5765' AS label;
UPDATE bench_t_6 SET payload = 5766 WHERE id = 6;
# hash comment 5767
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
$dz$ dollar body 5769 ; semicolon inside $dz$
SELECT 5770 AS id, 'row_5770' AS label;
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
WITH cte_5772 AS (SELECT 5772 AS n) SELECT n FROM cte_5772;
-- line 5773: deterministic comment
INSERT INTO bench_t_14 (id, payload) VALUES (5774, 'v5774');
# hash comment 5775
/* block header 5776 */
SELECT 5777 AS id, 'row_5777' AS label;
# hash comment 5778
SELECT nested FROM t WHERE id IN (5779, 5780, 5781);
WITH cte_5780 AS (SELECT 5780 AS n) SELECT n FROM cte_5780;
/* block header 5781 */
WITH cte_5782 AS (SELECT 5782 AS n) SELECT n FROM cte_5782;
SELECT 5783 AS id, 'row_5783' AS label;
SELECT * FROM "quoted_5784" WHERE col = E'esc\'5784';
UPDATE bench_t_25 SET payload = 5785 WHERE id = 25;
/* block header 5786 */
SELECT * FROM "quoted_5787" WHERE col = E'esc\'5787';
SELECT `mysql_5788` FROM `tbl_38`;
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT `mysql_5790` FROM `tbl_40`;
UPDATE bench_t_31 SET payload = 5791 WHERE id = 31;
SELECT * FROM "quoted_5792" WHERE col = E'esc\'5792';
SELECT [bracket_5793] FROM [dbo].[tbl_33];
SELECT nested FROM t WHERE id IN (5794, 5795, 5796);
SELECT * FROM "quoted_5795" WHERE col = E'esc\'5795';
SELECT 5796 AS id, 'row_5796' AS label;
UPDATE bench_t_37 SET payload = 5797 WHERE id = 5;
$dz$ dollar body 5798 ; semicolon inside $dz$
DELETE FROM bench_t_7 WHERE id = 7;
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
WITH cte_5801 AS (SELECT 5801 AS n) SELECT n FROM cte_5801;
SELECT `mysql_5802` FROM `tbl_2`;
SELECT * FROM "quoted_5803" WHERE col = E'esc\'5803';
/* block header 5804 */
SELECT * FROM "quoted_5805" WHERE col = E'esc\'5805';
SELECT `mysql_5806` FROM `tbl_6`;
SELECT `mysql_5807` FROM `tbl_7`;
# hash comment 5808
$dz$ dollar body 5809 ; semicolon inside $dz$
SELECT nested FROM t WHERE id IN (5810, 5811, 5812);
SELECT `mysql_5811` FROM `tbl_11`;
SELECT 5812 AS id, 'row_5812' AS label;
SELECT `mysql_5813` FROM `tbl_13`;
BEGIN; SELECT 5814; COMMIT;
BEGIN; SELECT 5815; COMMIT;
SELECT 5816 AS id, 'row_5816' AS label;
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
/* block header 5818 */
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
/* block header 5820 */
# hash comment 5821
INSERT INTO bench_t_62 (id, payload) VALUES (5822, 'v5822');
SELECT * FROM "quoted_5823" WHERE col = E'esc\'5823';
# hash comment 5824
SELECT 5825 AS id, 'row_5825' AS label;
SELECT 5826 AS id, 'row_5826' AS label;
BEGIN; SELECT 5827; COMMIT;
SELECT 5828 AS id, 'row_5828' AS label;
-- line 5829: deterministic comment
SELECT `mysql_5830` FROM `tbl_30`;
INSERT INTO bench_t_71 (id, payload) VALUES (5831, 'v5831');
/* block header 5832 */
SELECT `mysql_5833` FROM `tbl_33`;
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
# hash comment 5835
SELECT 5836 AS id, 'row_5836' AS label;
# hash comment 5837
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
DELETE FROM bench_t_15 WHERE id = 15;
/* block header 5840 */
WITH cte_5841 AS (SELECT 5841 AS n) SELECT n FROM cte_5841;
UPDATE bench_t_18 SET payload = 5842 WHERE id = 18;
# hash comment 5843
# hash comment 5844
DELETE FROM bench_t_21 WHERE id = 5;
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
WITH cte_5847 AS (SELECT 5847 AS n) SELECT n FROM cte_5847;
WITH cte_5848 AS (SELECT 5848 AS n) SELECT n FROM cte_5848;
-- line 5849: deterministic comment
UPDATE bench_t_26 SET payload = 5850 WHERE id = 26;
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
INSERT INTO bench_t_92 (id, payload) VALUES (5852, 'O''Brien');
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT * FROM "quoted_5854" WHERE col = E'esc\'5854';
/* block header 5855 */
-- line 5856: deterministic comment
$dz$ dollar body 5857 ; semicolon inside $dz$
DELETE FROM bench_t_2 WHERE id = 2;
WITH cte_5859 AS (SELECT 5859 AS n) SELECT n FROM cte_5859;
SELECT nested FROM t WHERE id IN (5860, 5861, 5862);
/* block header 5861 */
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
-- line 5863: deterministic comment
SELECT [bracket_5864] FROM [dbo].[tbl_24];
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT nested FROM t WHERE id IN (5866, 5867, 5868);
DELETE FROM bench_t_11 WHERE id = 11;
WITH cte_5868 AS (SELECT 5868 AS n) SELECT n FROM cte_5868;
/* block header 5869 */
SELECT [bracket_5870] FROM [dbo].[tbl_30];
SELECT 5871 AS id, 'row_5871' AS label;
WITH cte_5872 AS (SELECT 5872 AS n) SELECT n FROM cte_5872;
# hash comment 5873
UPDATE bench_t_50 SET payload = 5874 WHERE id = 18;
SELECT [bracket_5875] FROM [dbo].[tbl_35];
SELECT nested FROM t WHERE id IN (5876, 5877, 5878);
WITH cte_5877 AS (SELECT 5877 AS n) SELECT n FROM cte_5877;
-- line 5878: deterministic comment
UPDATE bench_t_55 SET payload = 5879 WHERE id = 23;
INSERT INTO bench_t_120 (id, payload) VALUES (5880, 'v5880');
DELETE FROM bench_t_25 WHERE id = 9;
SELECT [bracket_5882] FROM [dbo].[tbl_2];
SELECT `mysql_5883` FROM `tbl_33`;
BEGIN; SELECT 5884; COMMIT;
-- line 5885: deterministic comment
INSERT INTO bench_t_126 (id, payload) VALUES (5886, 'v5886');
SELECT 5887 AS id, 'row_5887' AS label;
DELETE FROM bench_t_0 WHERE id = 0;
/* block header 5889 */
BEGIN; SELECT 5890; COMMIT;
UPDATE bench_t_3 SET payload = 5891 WHERE id = 3;
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT `mysql_5893` FROM `tbl_43`;
UPDATE bench_t_6 SET payload = 5894 WHERE id = 6;
SELECT nested FROM t WHERE id IN (5895, 5896, 5897);
-- line 5896: deterministic comment
WITH cte_5897 AS (SELECT 5897 AS n) SELECT n FROM cte_5897;
UPDATE bench_t_10 SET payload = 5898 WHERE id = 10;
SELECT [bracket_5899] FROM [dbo].[tbl_19];
SELECT nested FROM t WHERE id IN (5900, 5901, 5902);
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
UPDATE bench_t_14 SET payload = 5902 WHERE id = 14;
$dz$ dollar body 5903 ; semicolon inside $dz$
# hash comment 5904
INSERT INTO bench_t_17 (id, payload) VALUES (5905, 'v5905');
BEGIN; SELECT 5906; COMMIT;
BEGIN; SELECT 5907; COMMIT;
UPDATE bench_t_20 SET payload = 5908 WHERE id = 20;
WITH cte_5909 AS (SELECT 5909 AS n) SELECT n FROM cte_5909;
INSERT INTO bench_t_22 (id, payload) VALUES (5910, 'v5910');
$dz$ dollar body 5911 ; semicolon inside $dz$
UPDATE bench_t_24 SET payload = 5912 WHERE id = 24;
INSERT INTO bench_t_25 (id, payload) VALUES (5913, 'v5913');
SELECT * FROM "quoted_5914" WHERE col = E'esc\'5914';
SELECT 5915 AS id, 'row_5915' AS label;
DELETE FROM bench_t_28 WHERE id = 12;
SELECT * FROM "quoted_5917" WHERE col = E'esc\'5917';
WITH cte_5918 AS (SELECT 5918 AS n) SELECT n FROM cte_5918;
BEGIN; SELECT 5919; COMMIT;
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
$dz$ dollar body 5921 ; semicolon inside $dz$
DELETE FROM bench_t_2 WHERE id = 2;
SELECT nested FROM t WHERE id IN (5923, 5924, 5925);
/* block header 5924 */
SELECT nested FROM t WHERE id IN (5925, 5926, 5927);
DELETE FROM bench_t_6 WHERE id = 6;
$dz$ dollar body 5927 ; semicolon inside $dz$
SELECT nested FROM t WHERE id IN (5928, 5929, 5930);
$dz$ dollar body 5929 ; semicolon inside $dz$
# hash comment 5930
$dz$ dollar body 5931 ; semicolon inside $dz$
/* block header 5932 */
$dz$ dollar body 5933 ; semicolon inside $dz$
BEGIN; SELECT 5934; COMMIT;
-- line 5935: deterministic comment
# hash comment 5936
SELECT nested FROM t WHERE id IN (5937, 5938, 5939);
SELECT `mysql_5938` FROM `tbl_38`;
$dz$ dollar body 5939 ; semicolon inside $dz$
UPDATE bench_t_52 SET payload = 5940 WHERE id = 20;
SELECT * FROM "quoted_5941" WHERE col = E'esc\'5941';
# hash comment 5942
BEGIN; SELECT 5943; COMMIT;
SELECT `mysql_5944` FROM `tbl_44`;
DELETE FROM bench_t_25 WHERE id = 9;
WITH cte_5946 AS (SELECT 5946 AS n) SELECT n FROM cte_5946;
$dz$ dollar body 5947 ; semicolon inside $dz$
# hash comment 5948
SELECT `mysql_5949` FROM `tbl_49`;
SELECT * FROM "quoted_5950" WHERE col = E'esc\'5950';
$dz$ dollar body 5951 ; semicolon inside $dz$
INSERT INTO bench_t_64 (id, payload) VALUES (5952, 'v5952');
$dz$ dollar body 5953 ; semicolon inside $dz$
# hash comment 5954
-- line 5955: deterministic comment
SELECT * FROM "quoted_5956" WHERE col = E'esc\'5956';
/* block header 5957 */
UPDATE bench_t_6 SET payload = 5958 WHERE id = 6;
# hash comment 5959
# hash comment 5960
INSERT INTO bench_t_73 (id, payload) VALUES (5961, 'v5961');
BEGIN; SELECT 5962; COMMIT;
WITH cte_5963 AS (SELECT 5963 AS n) SELECT n FROM cte_5963;
SELECT [bracket_5964] FROM [dbo].[tbl_4];
SELECT 5965 AS id, 'row_5965' AS label;
DELETE FROM bench_t_14 WHERE id = 14;
SELECT * FROM "quoted_5967" WHERE col = E'esc\'5967';
# hash comment 5968
SELECT [bracket_5969] FROM [dbo].[tbl_9];
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT `mysql_5971` FROM `tbl_21`;
SELECT 5972 AS id, 'row_5972' AS label;
BEGIN; SELECT 5973; COMMIT;
$dz$ dollar body 5974 ; semicolon inside $dz$
DELETE FROM bench_t_23 WHERE id = 7;
BEGIN; SELECT 5976; COMMIT;
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
# hash comment 5979
SELECT [bracket_5980] FROM [dbo].[tbl_20];
/* block header 5981 */
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
/* block header 5983 */
SELECT `mysql_5984` FROM `tbl_34`;
$dz$ dollar body 5985 ; semicolon inside $dz$
SELECT [bracket_5986] FROM [dbo].[tbl_26];
WITH cte_5987 AS (SELECT 5987 AS n) SELECT n FROM cte_5987;
SELECT 5988 AS id, 'row_5988' AS label;
UPDATE bench_t_37 SET payload = 5989 WHERE id = 5;
WITH cte_5990 AS (SELECT 5990 AS n) SELECT n FROM cte_5990;
SELECT * FROM "quoted_5991" WHERE col = E'esc\'5991';
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
UPDATE bench_t_41 SET payload = 5993 WHERE id = 9;
SELECT `mysql_5994` FROM `tbl_44`;
BEGIN; SELECT 5995; COMMIT;
$dz$ dollar body 5996 ; semicolon inside $dz$
/* block header 5997 */
BEGIN; SELECT 5998; COMMIT;
INSERT INTO bench_t_111 (id, payload) VALUES (5999, 'v5999');
/*
 * section 24
 * checksum 93b4
 */
SELECT [bracket_6000] FROM [dbo].[tbl_0];
BEGIN; SELECT 6005; COMMIT;
SELECT nested FROM t WHERE id IN (6006, 6007, 6008);
BEGIN; SELECT 6007; COMMIT;
SELECT `mysql_6008` FROM `tbl_8`;
SELECT `mysql_6009` FROM `tbl_9`;
SELECT [bracket_6010] FROM [dbo].[tbl_10];
SELECT 6011 AS id, 'row_6011' AS label;
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
DELETE FROM bench_t_29 WHERE id = 13;
SELECT [bracket_6014] FROM [dbo].[tbl_14];
SELECT 6015 AS id, 'row_6015' AS label;
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT [bracket_6018] FROM [dbo].[tbl_18];
-- line 6019: deterministic comment
$dz$ dollar body 6020 ; semicolon inside $dz$
BEGIN; SELECT 6021; COMMIT;
-- line 6022: deterministic comment
SELECT nested FROM t WHERE id IN (6023, 6024, 6025);
-- line 6024: deterministic comment
SELECT * FROM "quoted_6025" WHERE col = E'esc\'6025';
INSERT INTO bench_t_10 (id, payload) VALUES (6026, 'v6026');
/* block header 6027 */
-- line 6028: deterministic comment
WITH cte_6029 AS (SELECT 6029 AS n) SELECT n FROM cte_6029;
UPDATE bench_t_14 SET payload = 6030 WHERE id = 14;
SELECT [bracket_6031] FROM [dbo].[tbl_31];
UPDATE bench_t_16 SET payload = 6032 WHERE id = 16;
SELECT nested FROM t WHERE id IN (6033, 6034, 6035);
# hash comment 6034
SELECT nested FROM t WHERE id IN (6035, 6036, 6037);
SELECT nested FROM t WHERE id IN (6036, 6037, 6038);
# hash comment 6037
$dz$ dollar body 6038 ; semicolon inside $dz$
DELETE FROM bench_t_23 WHERE id = 7;
INSERT INTO bench_t_24 (id, payload) VALUES (6040, 'v6040');
SELECT [bracket_6041] FROM [dbo].[tbl_1];
# hash comment 6042
WITH cte_6043 AS (SELECT 6043 AS n) SELECT n FROM cte_6043;
# hash comment 6044
# hash comment 6045
SELECT `mysql_6046` FROM `tbl_46`;
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
DELETE FROM bench_t_0 WHERE id = 0;
/* block header 6049 */
INSERT INTO bench_t_34 (id, payload) VALUES (6050, 'O''Brien');
UPDATE bench_t_35 SET payload = 6051 WHERE id = 3;
WITH cte_6052 AS (SELECT 6052 AS n) SELECT n FROM cte_6052;
-- line 6053: deterministic comment
SELECT nested FROM t WHERE id IN (6054, 6055, 6056);
SELECT * FROM "quoted_6055" WHERE col = E'esc\'6055';
SELECT * FROM "quoted_6056" WHERE col = E'esc\'6056';
DELETE FROM bench_t_9 WHERE id = 9;
$dz$ dollar body 6058 ; semicolon inside $dz$
SELECT [bracket_6059] FROM [dbo].[tbl_19];
# hash comment 6060
INSERT INTO bench_t_45 (id, payload) VALUES (6061, 'O''Brien');
$dz$ dollar body 6062 ; semicolon inside $dz$
INSERT INTO bench_t_47 (id, payload) VALUES (6063, 'v6063');
SELECT * FROM "quoted_6064" WHERE col = E'esc\'6064';
-- line 6065: deterministic comment
# hash comment 6066
INSERT INTO bench_t_51 (id, payload) VALUES (6067, 'v6067');
# hash comment 6068
SELECT * FROM "quoted_6069" WHERE col = E'esc\'6069';
# hash comment 6070
# hash comment 6071
/* block header 6072 */
UPDATE bench_t_57 SET payload = 6073 WHERE id = 25;
SELECT 6074 AS id, 'row_6074' AS label;
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
# hash comment 6076
# hash comment 6077
SELECT 6078 AS id, 'row_6078' AS label;
UPDATE bench_t_63 SET payload = 6079 WHERE id = 31;
DELETE FROM bench_t_0 WHERE id = 0;
SELECT `mysql_6081` FROM `tbl_31`;
SELECT `mysql_6082` FROM `tbl_32`;
SELECT `mysql_6083` FROM `tbl_33`;
SELECT `mysql_6084` FROM `tbl_34`;
SELECT nested FROM t WHERE id IN (6085, 6086, 6087);
INSERT INTO bench_t_70 (id, payload) VALUES (6086, 'v6086');
SELECT `mysql_6087` FROM `tbl_37`;
UPDATE bench_t_8 SET payload = 6088 WHERE id = 8;
# hash comment 6089
$dz$ dollar body 6090 ; semicolon inside $dz$
BEGIN; SELECT 6091; COMMIT;
SELECT `mysql_6092` FROM `tbl_42`;
UPDATE bench_t_13 SET payload = 6093 WHERE id = 13;
$dz$ dollar body 6094 ; semicolon inside $dz$
# hash comment 6095
SELECT 6096 AS id, 'row_6096' AS label;
SELECT nested FROM t WHERE id IN (6097, 6098, 6099);
-- line 6098: deterministic comment
SELECT 6099 AS id, 'row_6099' AS label;
SELECT `mysql_6100` FROM `tbl_0`;
SELECT [bracket_6101] FROM [dbo].[tbl_21];
BEGIN; SELECT 6102; COMMIT;
# hash comment 6103
$dz$ dollar body 6104 ; semicolon inside $dz$
/* block header 6105 */
BEGIN; SELECT 6106; COMMIT;
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT [bracket_6109] FROM [dbo].[tbl_29];
# hash comment 6110
/* block header 6111 */
SELECT nested FROM t WHERE id IN (6112, 6113, 6114);
# hash comment 6113
-- line 6114: deterministic comment
$dz$ dollar body 6115 ; semicolon inside $dz$
SELECT * FROM "quoted_6116" WHERE col = E'esc\'6116';
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
$dz$ dollar body 6118 ; semicolon inside $dz$
SELECT nested FROM t WHERE id IN (6119, 6120, 6121);
$dz$ dollar body 6120 ; semicolon inside $dz$
SELECT 6121 AS id, 'row_6121' AS label;
SELECT nested FROM t WHERE id IN (6122, 6123, 6124);
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT nested FROM t WHERE id IN (6124, 6125, 6126);
SELECT [bracket_6125] FROM [dbo].[tbl_5];
BEGIN; SELECT 6126; COMMIT;
SELECT * FROM "quoted_6127" WHERE col = E'esc\'6127';
UPDATE bench_t_48 SET payload = 6128 WHERE id = 16;
# hash comment 6129
UPDATE bench_t_50 SET payload = 6130 WHERE id = 18;
SELECT [bracket_6131] FROM [dbo].[tbl_11];
SELECT `mysql_6132` FROM `tbl_32`;
-- line 6133: deterministic comment
-- line 6134: deterministic comment
BEGIN; SELECT 6135; COMMIT;
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT nested FROM t WHERE id IN (6137, 6138, 6139);
WITH cte_6138 AS (SELECT 6138 AS n) SELECT n FROM cte_6138;
UPDATE bench_t_59 SET payload = 6139 WHERE id = 27;
SELECT 6140 AS id, 'row_6140' AS label;
SELECT `mysql_6141` FROM `tbl_41`;
SELECT 6142 AS id, 'row_6142' AS label;
INSERT INTO bench_t_127 (id, payload) VALUES (6143, 'v6143');
UPDATE bench_t_0 SET payload = 6144 WHERE id = 0;
SELECT 6145 AS id, 'row_6145' AS label;
SELECT `mysql_6146` FROM `tbl_46`;
SELECT [bracket_6147] FROM [dbo].[tbl_27];
WITH cte_6148 AS (SELECT 6148 AS n) SELECT n FROM cte_6148;
$dz$ dollar body 6149 ; semicolon inside $dz$
# hash comment 6150
$dz$ dollar body 6151 ; semicolon inside $dz$
DELETE FROM bench_t_8 WHERE id = 8;
SELECT nested FROM t WHERE id IN (6153, 6154, 6155);
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT `mysql_6155` FROM `tbl_5`;
INSERT INTO bench_t_12 (id, payload) VALUES (6156, 'v6156');
/* block header 6157 */
SELECT 6158 AS id, 'row_6158' AS label;
SELECT `mysql_6159` FROM `tbl_9`;
SELECT `mysql_6160` FROM `tbl_10`;
WITH cte_6161 AS (SELECT 6161 AS n) SELECT n FROM cte_6161;
$dz$ dollar body 6162 ; semicolon inside $dz$
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT 6164 AS id, 'row_6164' AS label;
SELECT [bracket_6165] FROM [dbo].[tbl_5];
INSERT INTO bench_t_22 (id, payload) VALUES (6166, 'v6166');
SELECT 6167 AS id, 'row_6167' AS label;
# hash comment 6168
SELECT `mysql_6169` FROM `tbl_19`;
SELECT 6170 AS id, 'row_6170' AS label;
SELECT `mysql_6171` FROM `tbl_21`;
DELETE FROM bench_t_28 WHERE id = 12;
# hash comment 6173
-- line 6174: deterministic comment
/* block header 6175 */
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
# hash comment 6177
WITH cte_6178 AS (SELECT 6178 AS n) SELECT n FROM cte_6178;
-- line 6179: deterministic comment
DELETE FROM bench_t_4 WHERE id = 4;
DELETE FROM bench_t_5 WHERE id = 5;
INSERT INTO bench_t_38 (id, payload) VALUES (6182, 'O''Brien');
$dz$ dollar body 6183 ; semicolon inside $dz$
SELECT * FROM "quoted_6184" WHERE col = E'esc\'6184';
BEGIN; SELECT 6185; COMMIT;
SELECT 6186 AS id, 'row_6186' AS label;
SELECT 6187 AS id, 'row_6187' AS label;
-- line 6188: deterministic comment
SELECT `mysql_6189` FROM `tbl_39`;
SELECT [bracket_6190] FROM [dbo].[tbl_30];
/* block header 6191 */
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
INSERT INTO bench_t_49 (id, payload) VALUES (6193, 'O''Brien');
/* block header 6194 */
SELECT `mysql_6195` FROM `tbl_45`;
SELECT `mysql_6196` FROM `tbl_46`;
SELECT nested FROM t WHERE id IN (6197, 6198, 6199);
DELETE FROM bench_t_22 WHERE id = 6;
SELECT nested FROM t WHERE id IN (6199, 6200, 6201);
BEGIN; SELECT 6200; COMMIT;
/* block header 6201 */
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
DELETE FROM bench_t_28 WHERE id = 12;
-- line 6205: deterministic comment
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
BEGIN; SELECT 6207; COMMIT;
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
DELETE FROM bench_t_1 WHERE id = 1;
$dz$ dollar body 6210 ; semicolon inside $dz$
SELECT [bracket_6211] FROM [dbo].[tbl_11];
/* block header 6212 */
-- line 6213: deterministic comment
DELETE FROM bench_t_6 WHERE id = 6;
-- line 6215: deterministic comment
SELECT nested FROM t WHERE id IN (6216, 6217, 6218);
UPDATE bench_t_9 SET payload = 6217 WHERE id = 9;
SELECT [bracket_6218] FROM [dbo].[tbl_18];
-- line 6219: deterministic comment
SELECT 6220 AS id, 'row_6220' AS label;
$dz$ dollar body 6221 ; semicolon inside $dz$
SELECT [bracket_6222] FROM [dbo].[tbl_22];
BEGIN; SELECT 6223; COMMIT;
WITH cte_6224 AS (SELECT 6224 AS n) SELECT n FROM cte_6224;
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
UPDATE bench_t_18 SET payload = 6226 WHERE id = 18;
SELECT `mysql_6227` FROM `tbl_27`;
SELECT `mysql_6228` FROM `tbl_28`;
WITH cte_6229 AS (SELECT 6229 AS n) SELECT n FROM cte_6229;
SELECT `mysql_6230` FROM `tbl_30`;
INSERT INTO bench_t_87 (id, payload) VALUES (6231, 'v6231');
SELECT `mysql_6232` FROM `tbl_32`;
SELECT 6233 AS id, 'row_6233' AS label;
# hash comment 6234
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
DELETE FROM bench_t_28 WHERE id = 12;
SELECT 6237 AS id, 'row_6237' AS label;
DELETE FROM bench_t_30 WHERE id = 14;
SELECT * FROM "quoted_6239" WHERE col = E'esc\'6239';
DELETE FROM bench_t_0 WHERE id = 0;
$dz$ dollar body 6241 ; semicolon inside $dz$
SELECT `mysql_6242` FROM `tbl_42`;
SELECT `mysql_6243` FROM `tbl_43`;
SELECT [bracket_6244] FROM [dbo].[tbl_4];
SELECT `mysql_6245` FROM `tbl_45`;
UPDATE bench_t_38 SET payload = 6246 WHERE id = 6;
BEGIN; SELECT 6247; COMMIT;
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
/* block header 6249 */
/*
 * section 25
 * checksum 7ee5
 */
DELETE FROM bench_t_10 WHERE id = 10;
DELETE FROM bench_t_15 WHERE id = 15;
-- line 6256: deterministic comment
DELETE FROM bench_t_17 WHERE id = 1;
INSERT INTO bench_t_114 (id, payload) VALUES (6258, 'v6258');
DELETE FROM bench_t_19 WHERE id = 3;
SELECT * FROM "quoted_6260" WHERE col = E'esc\'6260';
SELECT * FROM "quoted_6261" WHERE col = E'esc\'6261';
INSERT INTO bench_t_118 (id, payload) VALUES (6262, 'v6262');
# hash comment 6263
$dz$ dollar body 6264 ; semicolon inside $dz$
SELECT 6265 AS id, 'row_6265' AS label;
SELECT 6266 AS id, 'row_6266' AS label;
$dz$ dollar body 6267 ; semicolon inside $dz$
SELECT 6268 AS id, 'row_6268' AS label;
SELECT 6269 AS id, 'row_6269' AS label;
SELECT [bracket_6270] FROM [dbo].[tbl_30];
WITH cte_6271 AS (SELECT 6271 AS n) SELECT n FROM cte_6271;
DELETE FROM bench_t_0 WHERE id = 0;
SELECT [bracket_6273] FROM [dbo].[tbl_33];
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
/* block header 6275 */
INSERT INTO bench_t_4 (id, payload) VALUES (6276, 'v6276');
SELECT [bracket_6277] FROM [dbo].[tbl_37];
BEGIN; SELECT 6278; COMMIT;
SELECT nested FROM t WHERE id IN (6279, 6280, 6281);
$dz$ dollar body 6280 ; semicolon inside $dz$
SELECT `mysql_6281` FROM `tbl_31`;
$dz$ dollar body 6282 ; semicolon inside $dz$
-- line 6283: deterministic comment
BEGIN; SELECT 6284; COMMIT;
SELECT * FROM "quoted_6285" WHERE col = E'esc\'6285';
INSERT INTO bench_t_14 (id, payload) VALUES (6286, 'v6286');
SELECT 6287 AS id, 'row_6287' AS label;
SELECT `mysql_6288` FROM `tbl_38`;
-- line 6289: deterministic comment
WITH cte_6290 AS (SELECT 6290 AS n) SELECT n FROM cte_6290;
INSERT INTO bench_t_19 (id, payload) VALUES (6291, 'v6291');
-- line 6292: deterministic comment
/* block header 6293 */
-- line 6294: deterministic comment
# hash comment 6295
INSERT INTO bench_t_24 (id, payload) VALUES (6296, 'v6296');
-- line 6297: deterministic comment
SELECT [bracket_6298] FROM [dbo].[tbl_18];
WITH cte_6299 AS (SELECT 6299 AS n) SELECT n FROM cte_6299;
WITH cte_6300 AS (SELECT 6300 AS n) SELECT n FROM cte_6300;
BEGIN; SELECT 6301; COMMIT;
SELECT `mysql_6302` FROM `tbl_2`;
-- line 6303: deterministic comment
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
DELETE FROM bench_t_1 WHERE id = 1;
SELECT nested FROM t WHERE id IN (6306, 6307, 6308);
/* block header 6307 */
WITH cte_6308 AS (SELECT 6308 AS n) SELECT n FROM cte_6308;
SELECT 6309 AS id, 'row_6309' AS label;
-- line 6310: deterministic comment
/* block header 6311 */
/* block header 6312 */
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT * FROM "quoted_6314" WHERE col = E'esc\'6314';
$dz$ dollar body 6315 ; semicolon inside $dz$
UPDATE bench_t_44 SET payload = 6316 WHERE id = 12;
SELECT 6317 AS id, 'row_6317' AS label;
# hash comment 6318
BEGIN; SELECT 6319; COMMIT;
SELECT `mysql_6320` FROM `tbl_20`;
SELECT `mysql_6321` FROM `tbl_21`;
SELECT nested FROM t WHERE id IN (6322, 6323, 6324);
SELECT `mysql_6323` FROM `tbl_23`;
BEGIN; SELECT 6324; COMMIT;
INSERT INTO bench_t_53 (id, payload) VALUES (6325, 'O''Brien');
/* block header 6326 */
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
BEGIN; SELECT 6328; COMMIT;
$dz$ dollar body 6329 ; semicolon inside $dz$
$dz$ dollar body 6330 ; semicolon inside $dz$
SELECT nested FROM t WHERE id IN (6331, 6332, 6333);
-- line 6332: deterministic comment
SELECT `mysql_6333` FROM `tbl_33`;
INSERT INTO bench_t_62 (id, payload) VALUES (6334, 'v6334');
DELETE FROM bench_t_31 WHERE id = 15;
$dz$ dollar body 6336 ; semicolon inside $dz$
INSERT INTO bench_t_65 (id, payload) VALUES (6337, 'v6337');
INSERT INTO bench_t_66 (id, payload) VALUES (6338, 'v6338');
SELECT 6339 AS id, 'row_6339' AS label;
-- line 6340: deterministic comment
# hash comment 6341
SELECT `mysql_6342` FROM `tbl_42`;
/* block header 6343 */
# hash comment 6344
INSERT INTO bench_t_73 (id, payload) VALUES (6345, 'v6345');
WITH cte_6346 AS (SELECT 6346 AS n) SELECT n FROM cte_6346;
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
-- line 6348: deterministic comment
WITH cte_6349 AS (SELECT 6349 AS n) SELECT n FROM cte_6349;
SELECT 6350 AS id, 'row_6350' AS label;
WITH cte_6351 AS (SELECT 6351 AS n) SELECT n FROM cte_6351;
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
WITH cte_6353 AS (SELECT 6353 AS n) SELECT n FROM cte_6353;
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
# hash comment 6355
SELECT `mysql_6356` FROM `tbl_6`;
SELECT * FROM "quoted_6357" WHERE col = E'esc\'6357';
SELECT [bracket_6358] FROM [dbo].[tbl_38];
SELECT `mysql_6359` FROM `tbl_9`;
DELETE FROM bench_t_24 WHERE id = 8;
SELECT * FROM "quoted_6361" WHERE col = E'esc\'6361';
SELECT `mysql_6362` FROM `tbl_12`;
$dz$ dollar body 6363 ; semicolon inside $dz$
INSERT INTO bench_t_92 (id, payload) VALUES (6364, 'v6364');
SELECT `mysql_6365` FROM `tbl_15`;
# hash comment 6366
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
# hash comment 6369
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
BEGIN; SELECT 6371; COMMIT;
-- line 6372: deterministic comment
UPDATE bench_t_37 SET payload = 6373 WHERE id = 5;
SELECT 6374 AS id, 'row_6374' AS label;
SELECT [bracket_6375] FROM [dbo].[tbl_15];
SELECT * FROM "quoted_6376" WHERE col = E'esc\'6376';
$dz$ dollar body 6377 ; semicolon inside $dz$
SELECT nested FROM t WHERE id IN (6378, 6379, 6380);
SELECT * FROM "quoted_6379" WHERE col = E'esc\'6379';
/* block header 6380 */
SELECT nested FROM t WHERE id IN (6381, 6382, 6383);
-- line 6382: deterministic comment
SELECT * FROM "quoted_6383" WHERE col = E'esc\'6383';
SELECT [bracket_6384] FROM [dbo].[tbl_24];
UPDATE bench_t_49 SET payload = 6385 WHERE id = 17;
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
INSERT INTO bench_t_115 (id, payload) VALUES (6387, 'v6387');
SELECT * FROM "quoted_6388" WHERE col = E'esc\'6388';
SELECT nested FROM t WHERE id IN (6389, 6390, 6391);
-- line 6390: deterministic comment
# hash comment 6391
SELECT nested FROM t WHERE id IN (6392, 6393, 6394);
SELECT [bracket_6393] FROM [dbo].[tbl_33];
SELECT `mysql_6394` FROM `tbl_44`;
INSERT INTO bench_t_123 (id, payload) VALUES (6395, 'v6395');
WITH cte_6396 AS (SELECT 6396 AS n) SELECT n FROM cte_6396;
INSERT INTO bench_t_125 (id, payload) VALUES (6397, 'v6397');
WITH cte_6398 AS (SELECT 6398 AS n) SELECT n FROM cte_6398;
# hash comment 6399
UPDATE bench_t_0 SET payload = 6400 WHERE id = 0;
$dz$ dollar body 6401 ; semicolon inside $dz$
/* block header 6402 */
SELECT [bracket_6403] FROM [dbo].[tbl_3];
/* block header 6404 */
SELECT * FROM "quoted_6405" WHERE col = E'esc\'6405';
INSERT INTO bench_t_6 (id, payload) VALUES (6406, 'v6406');
$dz$ dollar body 6407 ; semicolon inside $dz$
SELECT [bracket_6408] FROM [dbo].[tbl_8];
# hash comment 6409
SELECT [bracket_6410] FROM [dbo].[tbl_10];
WITH cte_6411 AS (SELECT 6411 AS n) SELECT n FROM cte_6411;
SELECT nested FROM t WHERE id IN (6412, 6413, 6414);
/* block header 6413 */
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT * FROM "quoted_6415" WHERE col = E'esc\'6415';
$dz$ dollar body 6416 ; semicolon inside $dz$
SELECT 6417 AS id, 'row_6417' AS label;
SELECT 6418 AS id, 'row_6418' AS label;
SELECT * FROM "quoted_6419" WHERE col = E'esc\'6419';
SELECT * FROM "quoted_6420" WHERE col = E'esc\'6420';
WITH cte_6421 AS (SELECT 6421 AS n) SELECT n FROM cte_6421;
INSERT INTO bench_t_22 (id, payload) VALUES (6422, 'v6422');
/* block header 6423 */
SELECT nested FROM t WHERE id IN (6424, 6425, 6426);
SELECT * FROM "quoted_6425" WHERE col = E'esc\'6425';
BEGIN; SELECT 6426; COMMIT;
SELECT nested FROM t WHERE id IN (6427, 6428, 6429);
SELECT nested FROM t WHERE id IN (6428, 6429, 6430);
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
DELETE FROM bench_t_30 WHERE id = 14;
INSERT INTO bench_t_31 (id, payload) VALUES (6431, 'v6431');
BEGIN; SELECT 6432; COMMIT;
BEGIN; SELECT 6433; COMMIT;
SELECT `mysql_6434` FROM `tbl_34`;
SELECT nested FROM t WHERE id IN (6435, 6436, 6437);
WITH cte_6436 AS (SELECT 6436 AS n) SELECT n FROM cte_6436;
INSERT INTO bench_t_37 (id, payload) VALUES (6437, 'v6437');
UPDATE bench_t_38 SET payload = 6438 WHERE id = 6;
SELECT `mysql_6439` FROM `tbl_39`;
# hash comment 6440
UPDATE bench_t_41 SET payload = 6441 WHERE id = 9;
SELECT * FROM "quoted_6442" WHERE col = E'esc\'6442';
$dz$ dollar body 6443 ; semicolon inside $dz$
WITH cte_6444 AS (SELECT 6444 AS n) SELECT n FROM cte_6444;
SELECT [bracket_6445] FROM [dbo].[tbl_5];
-- line 6446: deterministic comment
SELECT * FROM "quoted_6447" WHERE col = E'esc\'6447';
SELECT 6448 AS id, 'row_6448' AS label;
SELECT `mysql_6449` FROM `tbl_49`;
SELECT * FROM "quoted_6450" WHERE col = E'esc\'6450';
SELECT [bracket_6451] FROM [dbo].[tbl_11];
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
/* block header 6453 */
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
WITH cte_6456 AS (SELECT 6456 AS n) SELECT n FROM cte_6456;
SELECT nested FROM t WHERE id IN (6457, 6458, 6459);
INSERT INTO bench_t_58 (id, payload) VALUES (6458, 'v6458');
/* block header 6459 */
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
BEGIN; SELECT 6462; COMMIT;
SELECT nested FROM t WHERE id IN (6463, 6464, 6465);
BEGIN; SELECT 6464; COMMIT;
-- line 6465: deterministic comment
SELECT [bracket_6466] FROM [dbo].[tbl_26];
WITH cte_6467 AS (SELECT 6467 AS n) SELECT n FROM cte_6467;
SELECT `mysql_6468` FROM `tbl_18`;
WITH cte_6469 AS (SELECT 6469 AS n) SELECT n FROM cte_6469;
SELECT * FROM "quoted_6470" WHERE col = E'esc\'6470';
-- line 6471: deterministic comment
# hash comment 6472
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT 6474 AS id, 'row_6474' AS label;
SELECT * FROM "quoted_6475" WHERE col = E'esc\'6475';
/* block header 6476 */
# hash comment 6477
$dz$ dollar body 6478 ; semicolon inside $dz$
/* block header 6479 */
SELECT [bracket_6480] FROM [dbo].[tbl_0];
BEGIN; SELECT 6481; COMMIT;
SELECT 6482 AS id, 'row_6482' AS label;
UPDATE bench_t_19 SET payload = 6483 WHERE id = 19;
SELECT * FROM "quoted_6484" WHERE col = E'esc\'6484';
$dz$ dollar body 6485 ; semicolon inside $dz$
# hash comment 6486
$dz$ dollar body 6487 ; semicolon inside $dz$
$dz$ dollar body 6488 ; semicolon inside $dz$
SELECT 6489 AS id, 'row_6489' AS label;
# hash comment 6490
INSERT INTO bench_t_91 (id, payload) VALUES (6491, 'v6491');
INSERT INTO bench_t_92 (id, payload) VALUES (6492, 'v6492');
DELETE FROM bench_t_29 WHERE id = 13;
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
INSERT INTO bench_t_95 (id, payload) VALUES (6495, 'v6495');
# hash comment 6496
-- line 6497: deterministic comment
$dz$ dollar body 6498 ; semicolon inside $dz$
/* block header 6499 */
/*
 * section 26
 * checksum ea16
 */
SELECT 6500 AS id, 'row_6500' AS label;
SELECT [bracket_6505] FROM [dbo].[tbl_25];
SELECT nested FROM t WHERE id IN (6506, 6507, 6508);
SELECT 6507 AS id, 'row_6507' AS label;
DELETE FROM bench_t_12 WHERE id = 12;
DELETE FROM bench_t_13 WHERE id = 13;
/* block header 6510 */
INSERT INTO bench_t_111 (id, payload) VALUES (6511, 'v6511');
UPDATE bench_t_48 SET payload = 6512 WHERE id = 16;
SELECT nested FROM t WHERE id IN (6513, 6514, 6515);
SELECT nested FROM t WHERE id IN (6514, 6515, 6516);
/* block header 6515 */
SELECT nested FROM t WHERE id IN (6516, 6517, 6518);
BEGIN; SELECT 6517; COMMIT;
# hash comment 6518
/* block header 6519 */
# hash comment 6520
SELECT [bracket_6521] FROM [dbo].[tbl_1];
SELECT [bracket_6522] FROM [dbo].[tbl_2];
SELECT nested FROM t WHERE id IN (6523, 6524, 6525);
WITH cte_6524 AS (SELECT 6524 AS n) SELECT n FROM cte_6524;
DELETE FROM bench_t_29 WHERE id = 13;
$dz$ dollar body 6526 ; semicolon inside $dz$
SELECT * FROM "quoted_6527" WHERE col = E'esc\'6527';
$dz$ dollar body 6528 ; semicolon inside $dz$
SELECT `mysql_6529` FROM `tbl_29`;
WITH cte_6530 AS (SELECT 6530 AS n) SELECT n FROM cte_6530;
$dz$ dollar body 6531 ; semicolon inside $dz$
# hash comment 6532
WITH cte_6533 AS (SELECT 6533 AS n) SELECT n FROM cte_6533;
BEGIN; SELECT 6534; COMMIT;
SELECT nested FROM t WHERE id IN (6535, 6536, 6537);
SELECT nested FROM t WHERE id IN (6536, 6537, 6538);
SELECT `mysql_6537` FROM `tbl_37`;
SELECT `mysql_6538` FROM `tbl_38`;
-- line 6539: deterministic comment
$dz$ dollar body 6540 ; semicolon inside $dz$
/* block header 6541 */
-- line 6542: deterministic comment
# hash comment 6543
BEGIN; SELECT 6544; COMMIT;
DELETE FROM bench_t_17 WHERE id = 1;
BEGIN; SELECT 6546; COMMIT;
DELETE FROM bench_t_19 WHERE id = 3;
DELETE FROM bench_t_20 WHERE id = 4;
SELECT `mysql_6549` FROM `tbl_49`;
SELECT nested FROM t WHERE id IN (6550, 6551, 6552);
DELETE FROM bench_t_23 WHERE id = 7;
DELETE FROM bench_t_24 WHERE id = 8;
BEGIN; SELECT 6553; COMMIT;
-- line 6554: deterministic comment
WITH cte_6555 AS (SELECT 6555 AS n) SELECT n FROM cte_6555;
WITH cte_6556 AS (SELECT 6556 AS n) SELECT n FROM cte_6556;
SELECT * FROM "quoted_6557" WHERE col = E'esc\'6557';
WITH cte_6558 AS (SELECT 6558 AS n) SELECT n FROM cte_6558;
# hash comment 6559
-- line 6560: deterministic comment
-- line 6561: deterministic comment
-- line 6562: deterministic comment
# hash comment 6563
SELECT `mysql_6564` FROM `tbl_14`;
INSERT INTO bench_t_37 (id, payload) VALUES (6565, 'v6565');
UPDATE bench_t_38 SET payload = 6566 WHERE id = 6;
# hash comment 6567
$dz$ dollar body 6568 ; semicolon inside $dz$
SELECT nested FROM t WHERE id IN (6569, 6570, 6571);
BEGIN; SELECT 6570; COMMIT;
SELECT 6571 AS id, 'row_6571' AS label;
$dz$ dollar body 6572 ; semicolon inside $dz$
UPDATE bench_t_45 SET payload = 6573 WHERE id = 13;
SELECT 6574 AS id, 'row_6574' AS label;
UPDATE bench_t_47 SET payload = 6575 WHERE id = 15;
SELECT * FROM "quoted_6576" WHERE col = E'esc\'6576';
SELECT nested FROM t WHERE id IN (6577, 6578, 6579);
SELECT 6578 AS id, 'row_6578' AS label;
/* block header 6579 */
$dz$ dollar body 6580 ; semicolon inside $dz$
$dz$ dollar body 6581 ; semicolon inside $dz$
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT 6583 AS id, 'row_6583' AS label;
/* block header 6584 */
# hash comment 6585
SELECT * FROM "quoted_6586" WHERE col = E'esc\'6586';
INSERT INTO bench_t_59 (id, payload) VALUES (6587, 'v6587');
SELECT * FROM "quoted_6588" WHERE col = E'esc\'6588';
# hash comment 6589
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
-- line 6591: deterministic comment
SELECT nested FROM t WHERE id IN (6592, 6593, 6594);
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
DELETE FROM bench_t_2 WHERE id = 2;
DELETE FROM bench_t_3 WHERE id = 3;
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
UPDATE bench_t_5 SET payload = 6597 WHERE id = 5;
BEGIN; SELECT 6598; COMMIT;
$dz$ dollar body 6599 ; semicolon inside $dz$
SELECT nested FROM t WHERE id IN (6600, 6601, 6602);
INSERT INTO bench_t_73 (id, payload) VALUES (6601, 'v6601');
INSERT INTO bench_t_74 (id, payload) VALUES (6602, 'v6602');
WITH cte_6603 AS (SELECT 6603 AS n) SELECT n FROM cte_6603;
$dz$ dollar body 6604 ; semicolon inside $dz$
BEGIN; SELECT 6605; COMMIT;
SELECT [bracket_6606] FROM [dbo].[tbl_6];
-- line 6607: deterministic comment
-- line 6608: deterministic comment
# hash comment 6609
INSERT INTO bench_t_82 (id, payload) VALUES (6610, 'v6610');
BEGIN; SELECT 6611; COMMIT;
SELECT [bracket_6612] FROM [dbo].[tbl_12];
-- line 6613: deterministic comment
-- line 6614: deterministic comment
INSERT INTO bench_t_87 (id, payload) VALUES (6615, 'v6615');
SELECT 6616 AS id, 'row_6616' AS label;
# hash comment 6617
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT [bracket_6619] FROM [dbo].[tbl_19];
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
UPDATE bench_t_29 SET payload = 6621 WHERE id = 29;
SELECT * FROM "quoted_6622" WHERE col = E'esc\'6622';
# hash comment 6623
SELECT nested FROM t WHERE id IN (6624, 6625, 6626);
DELETE FROM bench_t_1 WHERE id = 1;
-- line 6626: deterministic comment
WITH cte_6627 AS (SELECT 6627 AS n) SELECT n FROM cte_6627;
DELETE FROM bench_t_4 WHERE id = 4;
SELECT * FROM "quoted_6629" WHERE col = E'esc\'6629';
/* block header 6630 */
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
$dz$ dollar body 6632 ; semicolon inside $dz$
# hash comment 6633
SELECT 6634 AS id, 'row_6634' AS label;
SELECT 6635 AS id, 'row_6635' AS label;
UPDATE bench_t_44 SET payload = 6636 WHERE id = 12;
SELECT 6637 AS id, 'row_6637' AS label;
WITH cte_6638 AS (SELECT 6638 AS n) SELECT n FROM cte_6638;
$dz$ dollar body 6639 ; semicolon inside $dz$
SELECT [bracket_6640] FROM [dbo].[tbl_0];
$dz$ dollar body 6641 ; semicolon inside $dz$
SELECT `mysql_6642` FROM `tbl_42`;
$dz$ dollar body 6643 ; semicolon inside $dz$
SELECT 6644 AS id, 'row_6644' AS label;
SELECT nested FROM t WHERE id IN (6645, 6646, 6647);
SELECT 6646 AS id, 'row_6646' AS label;
INSERT INTO bench_t_119 (id, payload) VALUES (6647, 'v6647');
SELECT * FROM "quoted_6648" WHERE col = E'esc\'6648';
DELETE FROM bench_t_25 WHERE id = 9;
BEGIN; SELECT 6650; COMMIT;
WITH cte_6651 AS (SELECT 6651 AS n) SELECT n FROM cte_6651;
# hash comment 6652
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
-- line 6654: deterministic comment
/* block header 6655 */
$dz$ dollar body 6656 ; semicolon inside $dz$
WITH cte_6657 AS (SELECT 6657 AS n) SELECT n FROM cte_6657;
SELECT [bracket_6658] FROM [dbo].[tbl_18];
WITH cte_6659 AS (SELECT 6659 AS n) SELECT n FROM cte_6659;
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
BEGIN; SELECT 6661; COMMIT;
WITH cte_6662 AS (SELECT 6662 AS n) SELECT n FROM cte_6662;
UPDATE bench_t_7 SET payload = 6663 WHERE id = 7;
$dz$ dollar body 6664 ; semicolon inside $dz$
WITH cte_6665 AS (SELECT 6665 AS n) SELECT n FROM cte_6665;
SELECT nested FROM t WHERE id IN (6666, 6667, 6668);
$dz$ dollar body 6667 ; semicolon inside $dz$
# hash comment 6668
BEGIN; SELECT 6669; COMMIT;
$dz$ dollar body 6670 ; semicolon inside $dz$
SELECT 6671 AS id, 'row_6671' AS label;
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
INSERT INTO bench_t_17 (id, payload) VALUES (6673, 'v6673');
SELECT `mysql_6674` FROM `tbl_24`;
SELECT [bracket_6675] FROM [dbo].[tbl_35];
BEGIN; SELECT 6676; COMMIT;
SELECT nested FROM t WHERE id IN (6677, 6678, 6679);
$dz$ dollar body 6678 ; semicolon inside $dz$
UPDATE bench_t_23 SET payload = 6679 WHERE id = 23;
SELECT * FROM "quoted_6680" WHERE col = E'esc\'6680';
WITH cte_6681 AS (SELECT 6681 AS n) SELECT n FROM cte_6681;
SELECT `mysql_6682` FROM `tbl_32`;
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
/* block header 6684 */
/* block header 6685 */
-- line 6686: deterministic comment
INSERT INTO bench_t_31 (id, payload) VALUES (6687, 'v6687');
INSERT INTO bench_t_32 (id, payload) VALUES (6688, 'O''Brien');
/* block header 6689 */
# hash comment 6690
$dz$ dollar body 6691 ; semicolon inside $dz$
SELECT [bracket_6692] FROM [dbo].[tbl_12];
$dz$ dollar body 6693 ; semicolon inside $dz$
# hash comment 6694
-- line 6695: deterministic comment
SELECT 6696 AS id, 'row_6696' AS label;
SELECT nested FROM t WHERE id IN (6697, 6698, 6699);
-- line 6698: deterministic comment
SELECT `mysql_6699` FROM `tbl_49`;
$dz$ dollar body 6700 ; semicolon inside $dz$
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT [bracket_6702] FROM [dbo].[tbl_22];
DELETE FROM bench_t_15 WHERE id = 15;
$dz$ dollar body 6704 ; semicolon inside $dz$
SELECT nested FROM t WHERE id IN (6705, 6706, 6707);
# hash comment 6706
SELECT `mysql_6707` FROM `tbl_7`;
BEGIN; SELECT 6708; COMMIT;
SELECT * FROM "quoted_6709" WHERE col = E'esc\'6709';
BEGIN; SELECT 6710; COMMIT;
SELECT * FROM "quoted_6711" WHERE col = E'esc\'6711';
SELECT * FROM "quoted_6712" WHERE col = E'esc\'6712';
BEGIN; SELECT 6713; COMMIT;
# hash comment 6714
$dz$ dollar body 6715 ; semicolon inside $dz$
WITH cte_6716 AS (SELECT 6716 AS n) SELECT n FROM cte_6716;
SELECT [bracket_6717] FROM [dbo].[tbl_37];
BEGIN; SELECT 6718; COMMIT;
SELECT [bracket_6719] FROM [dbo].[tbl_39];
DELETE FROM bench_t_0 WHERE id = 0;
SELECT * FROM "quoted_6721" WHERE col = E'esc\'6721';
-- line 6722: deterministic comment
INSERT INTO bench_t_67 (id, payload) VALUES (6723, 'v6723');
-- line 6724: deterministic comment
# hash comment 6725
UPDATE bench_t_6 SET payload = 6726 WHERE id = 6;
SELECT [bracket_6727] FROM [dbo].[tbl_7];
INSERT INTO bench_t_72 (id, payload) VALUES (6728, 'v6728');
SELECT 6729 AS id, 'row_6729' AS label;
UPDATE bench_t_10 SET payload = 6730 WHERE id = 10;
BEGIN; SELECT 6731; COMMIT;
WITH cte_6732 AS (SELECT 6732 AS n) SELECT n FROM cte_6732;
SELECT nested FROM t WHERE id IN (6733, 6734, 6735);
# hash comment 6734
WITH cte_6735 AS (SELECT 6735 AS n) SELECT n FROM cte_6735;
/* block header 6736 */
-- line 6737: deterministic comment
SELECT [bracket_6738] FROM [dbo].[tbl_18];
WITH cte_6739 AS (SELECT 6739 AS n) SELECT n FROM cte_6739;
SELECT `mysql_6740` FROM `tbl_40`;
DELETE FROM bench_t_21 WHERE id = 5;
SELECT nested FROM t WHERE id IN (6742, 6743, 6744);
INSERT INTO bench_t_87 (id, payload) VALUES (6743, 'O''Brien');
-- line 6744: deterministic comment
SELECT 6745 AS id, 'row_6745' AS label;
-- line 6746: deterministic comment
INSERT INTO bench_t_91 (id, payload) VALUES (6747, 'v6747');
# hash comment 6748
SELECT 6749 AS id, 'row_6749' AS label;
/*
 * section 27
 * checksum 8af7
 */
UPDATE bench_t_30 SET payload = 6750 WHERE id = 30;
SELECT [bracket_6755] FROM [dbo].[tbl_35];
UPDATE bench_t_36 SET payload = 6756 WHERE id = 4;
SELECT * FROM "quoted_6757" WHERE col = E'esc\'6757';
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT * FROM "quoted_6759" WHERE col = E'esc\'6759';
SELECT [bracket_6760] FROM [dbo].[tbl_0];
SELECT `mysql_6761` FROM `tbl_11`;
-- line 6762: deterministic comment
SELECT `mysql_6763` FROM `tbl_13`;
BEGIN; SELECT 6764; COMMIT;
$dz$ dollar body 6765 ; semicolon inside $dz$
SELECT 6766 AS id, 'row_6766' AS label;
UPDATE bench_t_47 SET payload = 6767 WHERE id = 15;
WITH cte_6768 AS (SELECT 6768 AS n) SELECT n FROM cte_6768;
SELECT [bracket_6769] FROM [dbo].[tbl_9];
$dz$ dollar body 6770 ; semicolon inside $dz$
WITH cte_6771 AS (SELECT 6771 AS n) SELECT n FROM cte_6771;
SELECT [bracket_6772] FROM [dbo].[tbl_12];
-- line 6773: deterministic comment
# hash comment 6774
-- line 6775: deterministic comment
UPDATE bench_t_56 SET payload = 6776 WHERE id = 24;
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
BEGIN; SELECT 6778; COMMIT;
BEGIN; SELECT 6779; COMMIT;
SELECT nested FROM t WHERE id IN (6780, 6781, 6782);
-- line 6781: deterministic comment
SELECT `mysql_6782` FROM `tbl_32`;
INSERT INTO bench_t_127 (id, payload) VALUES (6783, 'v6783');
SELECT nested FROM t WHERE id IN (6784, 6785, 6786);
SELECT [bracket_6785] FROM [dbo].[tbl_25];
SELECT * FROM "quoted_6786" WHERE col = E'esc\'6786';
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
INSERT INTO bench_t_4 (id, payload) VALUES (6788, 'v6788');
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT nested FROM t WHERE id IN (6790, 6791, 6792);
BEGIN; SELECT 6791; COMMIT;
INSERT INTO bench_t_8 (id, payload) VALUES (6792, 'v6792');
INSERT INTO bench_t_9 (id, payload) VALUES (6793, 'v6793');
SELECT * FROM "quoted_6794" WHERE col = E'esc\'6794';
UPDATE bench_t_11 SET payload = 6795 WHERE id = 11;
SELECT nested FROM t WHERE id IN (6796, 6797, 6798);
SELECT `mysql_6797` FROM `tbl_47`;
BEGIN; SELECT 6798; COMMIT;
SELECT 6799 AS id, 'row_6799' AS label;
WITH cte_6800 AS (SELECT 6800 AS n) SELECT n FROM cte_6800;
SELECT 6801 AS id, 'row_6801' AS label;
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT * FROM "quoted_6803" WHERE col = E'esc\'6803';
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
WITH cte_6805 AS (SELECT 6805 AS n) SELECT n FROM cte_6805;
SELECT 6806 AS id, 'row_6806' AS label;
BEGIN; SELECT 6807; COMMIT;
/* block header 6808 */
SELECT * FROM "quoted_6809" WHERE col = E'esc\'6809';
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
-- line 6811: deterministic comment
BEGIN; SELECT 6812; COMMIT;
SELECT `mysql_6813` FROM `tbl_13`;
/* block header 6814 */
SELECT 6815 AS id, 'row_6815' AS label;
DELETE FROM bench_t_0 WHERE id = 0;
INSERT INTO bench_t_33 (id, payload) VALUES (6817, 'v6817');
# hash comment 6818
WITH cte_6819 AS (SELECT 6819 AS n) SELECT n FROM cte_6819;
WITH cte_6820 AS (SELECT 6820 AS n) SELECT n FROM cte_6820;
SELECT nested FROM t WHERE id IN (6821, 6822, 6823);
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
$dz$ dollar body 6823 ; semicolon inside $dz$
SELECT 6824 AS id, 'row_6824' AS label;
# hash comment 6825
DELETE FROM bench_t_10 WHERE id = 10;
UPDATE bench_t_43 SET payload = 6827 WHERE id = 11;
INSERT INTO bench_t_44 (id, payload) VALUES (6828, 'v6828');
/* block header 6829 */
WITH cte_6830 AS (SELECT 6830 AS n) SELECT n FROM cte_6830;
SELECT 6831 AS id, 'row_6831' AS label;
SELECT `mysql_6832` FROM `tbl_32`;
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
-- line 6834: deterministic comment
SELECT 6835 AS id, 'row_6835' AS label;
SELECT 6836 AS id, 'row_6836' AS label;
DELETE FROM bench_t_21 WHERE id = 5;
-- line 6838: deterministic comment
SELECT [bracket_6839] FROM [dbo].[tbl_39];
WITH cte_6840 AS (SELECT 6840 AS n) SELECT n FROM cte_6840;
SELECT 6841 AS id, 'row_6841' AS label;
UPDATE bench_t_58 SET payload = 6842 WHERE id = 26;
INSERT INTO bench_t_59 (id, payload) VALUES (6843, 'v6843');
$dz$ dollar body 6844 ; semicolon inside $dz$
SELECT nested FROM t WHERE id IN (6845, 6846, 6847);
-- line 6846: deterministic comment
SELECT nested FROM t WHERE id IN (6847, 6848, 6849);
WITH cte_6848 AS (SELECT 6848 AS n) SELECT n FROM cte_6848;
# hash comment 6849
SELECT 6850 AS id, 'row_6850' AS label;
WITH cte_6851 AS (SELECT 6851 AS n) SELECT n FROM cte_6851;
WITH cte_6852 AS (SELECT 6852 AS n) SELECT n FROM cte_6852;
-- line 6853: deterministic comment
# hash comment 6854
UPDATE bench_t_7 SET payload = 6855 WHERE id = 7;
WITH cte_6856 AS (SELECT 6856 AS n) SELECT n FROM cte_6856;
SELECT 6857 AS id, 'row_6857' AS label;
/* block header 6858 */
DELETE FROM bench_t_11 WHERE id = 11;
BEGIN; SELECT 6860; COMMIT;
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
UPDATE bench_t_14 SET payload = 6862 WHERE id = 14;
# hash comment 6863
$dz$ dollar body 6864 ; semicolon inside $dz$
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT `mysql_6866` FROM `tbl_16`;
/* block header 6867 */
SELECT `mysql_6868` FROM `tbl_18`;
SELECT 6869 AS id, 'row_6869' AS label;
WITH cte_6870 AS (SELECT 6870 AS n) SELECT n FROM cte_6870;
# hash comment 6871
SELECT 6872 AS id, 'row_6872' AS label;
# hash comment 6873
BEGIN; SELECT 6874; COMMIT;
$dz$ dollar body 6875 ; semicolon inside $dz$
INSERT INTO bench_t_92 (id, payload) VALUES (6876, 'v6876');
DELETE FROM bench_t_29 WHERE id = 13;
SELECT nested FROM t WHERE id IN (6878, 6879, 6880);
SELECT `mysql_6879` FROM `tbl_29`;
SELECT * FROM "quoted_6880" WHERE col = E'esc\'6880';
-- line 6881: deterministic comment
$dz$ dollar body 6882 ; semicolon inside $dz$
WITH cte_6883 AS (SELECT 6883 AS n) SELECT n FROM cte_6883;
SELECT `mysql_6884` FROM `tbl_34`;
$dz$ dollar body 6885 ; semicolon inside $dz$
UPDATE bench_t_38 SET payload = 6886 WHERE id = 6;
SELECT nested FROM t WHERE id IN (6887, 6888, 6889);
SELECT * FROM "quoted_6888" WHERE col = E'esc\'6888';
SELECT `mysql_6889` FROM `tbl_39`;
SELECT nested FROM t WHERE id IN (6890, 6891, 6892);
SELECT * FROM "quoted_6891" WHERE col = E'esc\'6891';
-- line 6892: deterministic comment
-- line 6893: deterministic comment
$dz$ dollar body 6894 ; semicolon inside $dz$
/* block header 6895 */
SELECT nested FROM t WHERE id IN (6896, 6897, 6898);
SELECT `mysql_6897` FROM `tbl_47`;
SELECT * FROM "quoted_6898" WHERE col = E'esc\'6898';
SELECT [bracket_6899] FROM [dbo].[tbl_19];
-- line 6900: deterministic comment
INSERT INTO bench_t_117 (id, payload) VALUES (6901, 'v6901');
$dz$ dollar body 6902 ; semicolon inside $dz$
# hash comment 6903
/* block header 6904 */
# hash comment 6905
BEGIN; SELECT 6906; COMMIT;
SELECT [bracket_6907] FROM [dbo].[tbl_27];
# hash comment 6908
SELECT nested FROM t WHERE id IN (6909, 6910, 6911);
UPDATE bench_t_62 SET payload = 6910 WHERE id = 30;
UPDATE bench_t_63 SET payload = 6911 WHERE id = 31;
-- line 6912: deterministic comment
UPDATE bench_t_1 SET payload = 6913 WHERE id = 1;
SELECT 6914 AS id, 'row_6914' AS label;
DELETE FROM bench_t_3 WHERE id = 3;
WITH cte_6916 AS (SELECT 6916 AS n) SELECT n FROM cte_6916;
SELECT * FROM "quoted_6917" WHERE col = E'esc\'6917';
SELECT [bracket_6918] FROM [dbo].[tbl_38];
BEGIN; SELECT 6919; COMMIT;
-- line 6920: deterministic comment
SELECT nested FROM t WHERE id IN (6921, 6922, 6923);
INSERT INTO bench_t_10 (id, payload) VALUES (6922, 'v6922');
SELECT [bracket_6923] FROM [dbo].[tbl_3];
INSERT INTO bench_t_12 (id, payload) VALUES (6924, 'v6924');
WITH cte_6925 AS (SELECT 6925 AS n) SELECT n FROM cte_6925;
$dz$ dollar body 6926 ; semicolon inside $dz$
UPDATE bench_t_15 SET payload = 6927 WHERE id = 15;
SELECT 6928 AS id, 'row_6928' AS label;
/* block header 6929 */
INSERT INTO bench_t_18 (id, payload) VALUES (6930, 'O''Brien');
-- line 6931: deterministic comment
INSERT INTO bench_t_20 (id, payload) VALUES (6932, 'v6932');
DELETE FROM bench_t_21 WHERE id = 5;
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT * FROM "quoted_6935" WHERE col = E'esc\'6935';
INSERT INTO bench_t_24 (id, payload) VALUES (6936, 'v6936');
/* block header 6937 */
/* block header 6938 */
DELETE FROM bench_t_27 WHERE id = 11;
UPDATE bench_t_28 SET payload = 6940 WHERE id = 28;
/* block header 6941 */
-- line 6942: deterministic comment
SELECT nested FROM t WHERE id IN (6943, 6944, 6945);
SELECT 6944 AS id, 'row_6944' AS label;
UPDATE bench_t_33 SET payload = 6945 WHERE id = 1;
UPDATE bench_t_34 SET payload = 6946 WHERE id = 2;
SELECT nested FROM t WHERE id IN (6947, 6948, 6949);
UPDATE bench_t_36 SET payload = 6948 WHERE id = 4;
DELETE FROM bench_t_5 WHERE id = 5;
SELECT [bracket_6950] FROM [dbo].[tbl_30];
UPDATE bench_t_39 SET payload = 6951 WHERE id = 7;
BEGIN; SELECT 6952; COMMIT;
WITH cte_6953 AS (SELECT 6953 AS n) SELECT n FROM cte_6953;
WITH cte_6954 AS (SELECT 6954 AS n) SELECT n FROM cte_6954;
BEGIN; SELECT 6955; COMMIT;
-- line 6956: deterministic comment
BEGIN; SELECT 6957; COMMIT;
INSERT INTO bench_t_46 (id, payload) VALUES (6958, 'v6958');
# hash comment 6959
SELECT 6960 AS id, 'row_6960' AS label;
UPDATE bench_t_49 SET payload = 6961 WHERE id = 17;
WITH cte_6962 AS (SELECT 6962 AS n) SELECT n FROM cte_6962;
# hash comment 6963
-- line 6964: deterministic comment
DELETE FROM bench_t_21 WHERE id = 5;
SELECT `mysql_6966` FROM `tbl_16`;
SELECT `mysql_6967` FROM `tbl_17`;
DELETE FROM bench_t_24 WHERE id = 8;
SELECT 6969 AS id, 'row_6969' AS label;
SELECT 6970 AS id, 'row_6970' AS label;
$dz$ dollar body 6971 ; semicolon inside $dz$
SELECT * FROM "quoted_6972" WHERE col = E'esc\'6972';
WITH cte_6973 AS (SELECT 6973 AS n) SELECT n FROM cte_6973;
SELECT [bracket_6974] FROM [dbo].[tbl_14];
DELETE FROM bench_t_31 WHERE id = 15;
UPDATE bench_t_0 SET payload = 6976 WHERE id = 0;
SELECT [bracket_6977] FROM [dbo].[tbl_17];
UPDATE bench_t_2 SET payload = 6978 WHERE id = 2;
INSERT INTO bench_t_67 (id, payload) VALUES (6979, 'v6979');
DELETE FROM bench_t_4 WHERE id = 4;
WITH cte_6981 AS (SELECT 6981 AS n) SELECT n FROM cte_6981;
$dz$ dollar body 6982 ; semicolon inside $dz$
INSERT INTO bench_t_71 (id, payload) VALUES (6983, 'v6983');
INSERT INTO bench_t_72 (id, payload) VALUES (6984, 'v6984');
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT * FROM "quoted_6986" WHERE col = E'esc\'6986';
BEGIN; SELECT 6987; COMMIT;
SELECT nested FROM t WHERE id IN (6988, 6989, 6990);
-- line 6989: deterministic comment
INSERT INTO bench_t_78 (id, payload) VALUES (6990, 'v6990');
# hash comment 6991
# hash comment 6992
SELECT 6993 AS id, 'row_6993' AS label;
$dz$ dollar body 6994 ; semicolon inside $dz$
-- line 6995: deterministic comment
SELECT `mysql_6996` FROM `tbl_46`;
DELETE FROM bench_t_21 WHERE id = 5;
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
$dz$ dollar body 6999 ; semicolon inside $dz$
/*
 * section 28
 * checksum ba8a
 */
-- line 7000: deterministic comment
SELECT nested FROM t WHERE id IN (7005, 7006, 7007);
SELECT 7006 AS id, 'row_7006' AS label;
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT `mysql_7009` FROM `tbl_9`;
INSERT INTO bench_t_98 (id, payload) VALUES (7010, 'v7010');
SELECT * FROM "quoted_7011" WHERE col = E'esc\'7011';
WITH cte_7012 AS (SELECT 7012 AS n) SELECT n FROM cte_7012;
DELETE FROM bench_t_5 WHERE id = 5;
SELECT nested FROM t WHERE id IN (7014, 7015, 7016);
WITH cte_7015 AS (SELECT 7015 AS n) SELECT n FROM cte_7015;
BEGIN; SELECT 7016; COMMIT;
WITH cte_7017 AS (SELECT 7017 AS n) SELECT n FROM cte_7017;
/* block header 7018 */
SELECT * FROM "quoted_7019" WHERE col = E'esc\'7019';
SELECT [bracket_7020] FROM [dbo].[tbl_20];
UPDATE bench_t_45 SET payload = 7021 WHERE id = 13;
/* block header 7022 */
SELECT 7023 AS id, 'row_7023' AS label;
# hash comment 7024
UPDATE bench_t_49 SET payload = 7025 WHERE id = 17;
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
UPDATE bench_t_51 SET payload = 7027 WHERE id = 19;
-- line 7028: deterministic comment
INSERT INTO bench_t_117 (id, payload) VALUES (7029, 'O''Brien');
SELECT 7030 AS id, 'row_7030' AS label;
/* block header 7031 */
# hash comment 7032
# hash comment 7033
SELECT * FROM "quoted_7034" WHERE col = E'esc\'7034';
DELETE FROM bench_t_27 WHERE id = 11;
INSERT INTO bench_t_124 (id, payload) VALUES (7036, 'v7036');
WITH cte_7037 AS (SELECT 7037 AS n) SELECT n FROM cte_7037;
SELECT * FROM "quoted_7038" WHERE col = E'esc\'7038';
$dz$ dollar body 7039 ; semicolon inside $dz$
SELECT * FROM "quoted_7040" WHERE col = E'esc\'7040';
UPDATE bench_t_1 SET payload = 7041 WHERE id = 1;
UPDATE bench_t_2 SET payload = 7042 WHERE id = 2;
INSERT INTO bench_t_3 (id, payload) VALUES (7043, 'v7043');
SELECT * FROM "quoted_7044" WHERE col = E'esc\'7044';
# hash comment 7045
BEGIN; SELECT 7046; COMMIT;
-- line 7047: deterministic comment
SELECT nested FROM t WHERE id IN (7048, 7049, 7050);
-- line 7049: deterministic comment
SELECT * FROM "quoted_7050" WHERE col = E'esc\'7050';
$dz$ dollar body 7051 ; semicolon inside $dz$
SELECT 7052 AS id, 'row_7052' AS label;
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
/* block header 7054 */
$dz$ dollar body 7055 ; semicolon inside $dz$
BEGIN; SELECT 7056; COMMIT;
UPDATE bench_t_17 SET payload = 7057 WHERE id = 17;
UPDATE bench_t_18 SET payload = 7058 WHERE id = 18;
WITH cte_7059 AS (SELECT 7059 AS n) SELECT n FROM cte_7059;
SELECT * FROM "quoted_7060" WHERE col = E'esc\'7060';
$dz$ dollar body 7061 ; semicolon inside $dz$
SELECT * FROM "quoted_7062" WHERE col = E'esc\'7062';
SELECT nested FROM t WHERE id IN (7063, 7064, 7065);
UPDATE bench_t_24 SET payload = 7064 WHERE id = 24;
SELECT * FROM "quoted_7065" WHERE col = E'esc\'7065';
DELETE FROM bench_t_26 WHERE id = 10;
-- line 7067: deterministic comment
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
-- line 7069: deterministic comment
WITH cte_7070 AS (SELECT 7070 AS n) SELECT n FROM cte_7070;
UPDATE bench_t_31 SET payload = 7071 WHERE id = 31;
DELETE FROM bench_t_0 WHERE id = 0;
SELECT * FROM "quoted_7073" WHERE col = E'esc\'7073';
# hash comment 7074
$dz$ dollar body 7075 ; semicolon inside $dz$
DELETE FROM bench_t_4 WHERE id = 4;
SELECT * FROM "quoted_7077" WHERE col = E'esc\'7077';
DELETE FROM bench_t_6 WHERE id = 6;
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
$dz$ dollar body 7080 ; semicolon inside $dz$
DELETE FROM bench_t_9 WHERE id = 9;
/* block header 7082 */
WITH cte_7083 AS (SELECT 7083 AS n) SELECT n FROM cte_7083;
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT 7085 AS id, 'row_7085' AS label;
WITH cte_7086 AS (SELECT 7086 AS n) SELECT n FROM cte_7086;
DELETE FROM bench_t_15 WHERE id = 15;
SELECT nested FROM t WHERE id IN (7088, 7089, 7090);
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
WITH cte_7090 AS (SELECT 7090 AS n) SELECT n FROM cte_7090;
INSERT INTO bench_t_51 (id, payload) VALUES (7091, 'v7091');
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT * FROM "quoted_7093" WHERE col = E'esc\'7093';
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
WITH cte_7096 AS (SELECT 7096 AS n) SELECT n FROM cte_7096;
SELECT nested FROM t WHERE id IN (7097, 7098, 7099);
# hash comment 7098
DELETE FROM bench_t_27 WHERE id = 11;
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
UPDATE bench_t_61 SET payload = 7101 WHERE id = 29;
BEGIN; SELECT 7102; COMMIT;
# hash comment 7103
INSERT INTO bench_t_64 (id, payload) VALUES (7104, 'v7104');
INSERT INTO bench_t_65 (id, payload) VALUES (7105, 'v7105');
SELECT `mysql_7106` FROM `tbl_6`;
$dz$ dollar body 7107 ; semicolon inside $dz$
/* block header 7108 */
-- line 7109: deterministic comment
SELECT `mysql_7110` FROM `tbl_10`;
UPDATE bench_t_7 SET payload = 7111 WHERE id = 7;
UPDATE bench_t_8 SET payload = 7112 WHERE id = 8;
SELECT nested FROM t WHERE id IN (7113, 7114, 7115);
DELETE FROM bench_t_10 WHERE id = 10;
# hash comment 7115
SELECT [bracket_7116] FROM [dbo].[tbl_36];
DELETE FROM bench_t_13 WHERE id = 13;
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
-- line 7119: deterministic comment
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT * FROM "quoted_7121" WHERE col = E'esc\'7121';
SELECT [bracket_7122] FROM [dbo].[tbl_2];
-- line 7123: deterministic comment
SELECT * FROM "quoted_7124" WHERE col = E'esc\'7124';
DELETE FROM bench_t_21 WHERE id = 5;
WITH cte_7126 AS (SELECT 7126 AS n) SELECT n FROM cte_7126;
DELETE FROM bench_t_23 WHERE id = 7;
UPDATE bench_t_24 SET payload = 7128 WHERE id = 24;
# hash comment 7129
BEGIN; SELECT 7130; COMMIT;
/* block header 7131 */
INSERT INTO bench_t_92 (id, payload) VALUES (7132, 'v7132');
SELECT [bracket_7133] FROM [dbo].[tbl_13];
SELECT [bracket_7134] FROM [dbo].[tbl_14];
SELECT 7135 AS id, 'row_7135' AS label;
UPDATE bench_t_32 SET payload = 7136 WHERE id = 0;
/* block header 7137 */
UPDATE bench_t_34 SET payload = 7138 WHERE id = 2;
INSERT INTO bench_t_99 (id, payload) VALUES (7139, 'O''Brien');
SELECT `mysql_7140` FROM `tbl_40`;
SELECT [bracket_7141] FROM [dbo].[tbl_21];
SELECT `mysql_7142` FROM `tbl_42`;
WITH cte_7143 AS (SELECT 7143 AS n) SELECT n FROM cte_7143;
WITH cte_7144 AS (SELECT 7144 AS n) SELECT n FROM cte_7144;
UPDATE bench_t_41 SET payload = 7145 WHERE id = 9;
BEGIN; SELECT 7146; COMMIT;
BEGIN; SELECT 7147; COMMIT;
-- line 7148: deterministic comment
/* block header 7149 */
SELECT * FROM "quoted_7150" WHERE col = E'esc\'7150';
/* block header 7151 */
SELECT `mysql_7152` FROM `tbl_2`;
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
# hash comment 7154
$dz$ dollar body 7155 ; semicolon inside $dz$
# hash comment 7156
INSERT INTO bench_t_117 (id, payload) VALUES (7157, 'v7157');
DELETE FROM bench_t_22 WHERE id = 6;
DELETE FROM bench_t_23 WHERE id = 7;
SELECT [bracket_7160] FROM [dbo].[tbl_0];
UPDATE bench_t_57 SET payload = 7161 WHERE id = 25;
-- line 7162: deterministic comment
SELECT nested FROM t WHERE id IN (7163, 7164, 7165);
WITH cte_7164 AS (SELECT 7164 AS n) SELECT n FROM cte_7164;
SELECT [bracket_7165] FROM [dbo].[tbl_5];
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
# hash comment 7167
WITH cte_7168 AS (SELECT 7168 AS n) SELECT n FROM cte_7168;
SELECT `mysql_7169` FROM `tbl_19`;
SELECT [bracket_7170] FROM [dbo].[tbl_10];
$dz$ dollar body 7171 ; semicolon inside $dz$
SELECT 7172 AS id, 'row_7172' AS label;
SELECT * FROM "quoted_7173" WHERE col = E'esc\'7173';
-- line 7174: deterministic comment
-- line 7175: deterministic comment
SELECT `mysql_7176` FROM `tbl_26`;
WITH cte_7177 AS (SELECT 7177 AS n) SELECT n FROM cte_7177;
SELECT nested FROM t WHERE id IN (7178, 7179, 7180);
$dz$ dollar body 7179 ; semicolon inside $dz$
/* block header 7180 */
SELECT nested FROM t WHERE id IN (7181, 7182, 7183);
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT 7183 AS id, 'row_7183' AS label;
SELECT nested FROM t WHERE id IN (7184, 7185, 7186);
SELECT * FROM "quoted_7185" WHERE col = E'esc\'7185';
SELECT nested FROM t WHERE id IN (7186, 7187, 7188);
UPDATE bench_t_19 SET payload = 7187 WHERE id = 19;
SELECT `mysql_7188` FROM `tbl_38`;
INSERT INTO bench_t_21 (id, payload) VALUES (7189, 'v7189');
SELECT [bracket_7190] FROM [dbo].[tbl_30];
-- line 7191: deterministic comment
/* block header 7192 */
-- line 7193: deterministic comment
BEGIN; SELECT 7194; COMMIT;
SELECT `mysql_7195` FROM `tbl_45`;
SELECT 7196 AS id, 'row_7196' AS label;
# hash comment 7197
/* block header 7198 */
SELECT `mysql_7199` FROM `tbl_49`;
INSERT INTO bench_t_32 (id, payload) VALUES (7200, 'v7200');
SELECT * FROM "quoted_7201" WHERE col = E'esc\'7201';
UPDATE bench_t_34 SET payload = 7202 WHERE id = 2;
-- line 7203: deterministic comment
$dz$ dollar body 7204 ; semicolon inside $dz$
BEGIN; SELECT 7205; COMMIT;
SELECT `mysql_7206` FROM `tbl_6`;
SELECT nested FROM t WHERE id IN (7207, 7208, 7209);
# hash comment 7208
SELECT [bracket_7209] FROM [dbo].[tbl_9];
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
WITH cte_7211 AS (SELECT 7211 AS n) SELECT n FROM cte_7211;
SELECT `mysql_7212` FROM `tbl_12`;
SELECT [bracket_7213] FROM [dbo].[tbl_13];
UPDATE bench_t_46 SET payload = 7214 WHERE id = 14;
/* block header 7215 */
SELECT * FROM "quoted_7216" WHERE col = E'esc\'7216';
# hash comment 7217
SELECT * FROM "quoted_7218" WHERE col = E'esc\'7218';
SELECT [bracket_7219] FROM [dbo].[tbl_19];
# hash comment 7220
SELECT [bracket_7221] FROM [dbo].[tbl_21];
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT 7224 AS id, 'row_7224' AS label;
BEGIN; SELECT 7225; COMMIT;
WITH cte_7226 AS (SELECT 7226 AS n) SELECT n FROM cte_7226;
$dz$ dollar body 7227 ; semicolon inside $dz$
-- line 7228: deterministic comment
SELECT * FROM "quoted_7229" WHERE col = E'esc\'7229';
DELETE FROM bench_t_30 WHERE id = 14;
SELECT `mysql_7231` FROM `tbl_31`;
SELECT `mysql_7232` FROM `tbl_32`;
INSERT INTO bench_t_65 (id, payload) VALUES (7233, 'v7233');
SELECT [bracket_7234] FROM [dbo].[tbl_34];
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT nested FROM t WHERE id IN (7236, 7237, 7238);
# hash comment 7237
BEGIN; SELECT 7238; COMMIT;
INSERT INTO bench_t_71 (id, payload) VALUES (7239, 'v7239');
SELECT 7240 AS id, 'row_7240' AS label;
-- line 7241: deterministic comment
INSERT INTO bench_t_74 (id, payload) VALUES (7242, 'v7242');
BEGIN; SELECT 7243; COMMIT;
-- line 7244: deterministic comment
BEGIN; SELECT 7245; COMMIT;
SELECT * FROM "quoted_7246" WHERE col = E'esc\'7246';
INSERT INTO bench_t_79 (id, payload) VALUES (7247, 'v7247');
UPDATE bench_t_16 SET payload = 7248 WHERE id = 16;
SELECT * FROM "quoted_7249" WHERE col = E'esc\'7249';
/*
 * section 29
 * checksum e197
 */
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
DELETE FROM bench_t_23 WHERE id = 7;
SELECT nested FROM t WHERE id IN (7256, 7257, 7258);
-- line 7257: deterministic comment
UPDATE bench_t_26 SET payload = 7258 WHERE id = 26;
$dz$ dollar body 7259 ; semicolon inside $dz$
$dz$ dollar body 7260 ; semicolon inside $dz$
BEGIN; SELECT 7261; COMMIT;
SELECT * FROM "quoted_7262" WHERE col = E'esc\'7262';
SELECT `mysql_7263` FROM `tbl_13`;
DELETE FROM bench_t_0 WHERE id = 0;
INSERT INTO bench_t_97 (id, payload) VALUES (7265, 'v7265');
BEGIN; SELECT 7266; COMMIT;
BEGIN; SELECT 7267; COMMIT;
SELECT 7268 AS id, 'row_7268' AS label;
BEGIN; SELECT 7269; COMMIT;
SELECT 7270 AS id, 'row_7270' AS label;
SELECT [bracket_7271] FROM [dbo].[tbl_31];
-- line 7272: deterministic comment
BEGIN; SELECT 7273; COMMIT;
INSERT INTO bench_t_106 (id, payload) VALUES (7274, 'v7274');
BEGIN; SELECT 7275; COMMIT;
SELECT * FROM "quoted_7276" WHERE col = E'esc\'7276';
# hash comment 7277
SELECT [bracket_7278] FROM [dbo].[tbl_38];
DELETE FROM bench_t_15 WHERE id = 15;
SELECT [bracket_7280] FROM [dbo].[tbl_0];
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
INSERT INTO bench_t_114 (id, payload) VALUES (7282, 'O''Brien');
SELECT [bracket_7283] FROM [dbo].[tbl_3];
$dz$ dollar body 7284 ; semicolon inside $dz$
INSERT INTO bench_t_117 (id, payload) VALUES (7285, 'v7285');
SELECT * FROM "quoted_7286" WHERE col = E'esc\'7286';
SELECT [bracket_7287] FROM [dbo].[tbl_7];
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
/* block header 7289 */
SELECT nested FROM t WHERE id IN (7290, 7291, 7292);
INSERT INTO bench_t_123 (id, payload) VALUES (7291, 'v7291');
SELECT nested FROM t WHERE id IN (7292, 7293, 7294);
$dz$ dollar body 7293 ; semicolon inside $dz$
SELECT [bracket_7294] FROM [dbo].[tbl_14];
/* block header 7295 */
SELECT 7296 AS id, 'row_7296' AS label;
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
BEGIN; SELECT 7298; COMMIT;
WITH cte_7299 AS (SELECT 7299 AS n) SELECT n FROM cte_7299;
-- line 7300: deterministic comment
SELECT `mysql_7301` FROM `tbl_1`;
SELECT `mysql_7302` FROM `tbl_2`;
DELETE FROM bench_t_7 WHERE id = 7;
SELECT `mysql_7304` FROM `tbl_4`;
BEGIN; SELECT 7305; COMMIT;
DELETE FROM bench_t_10 WHERE id = 10;
UPDATE bench_t_11 SET payload = 7307 WHERE id = 11;
# hash comment 7308
SELECT nested FROM t WHERE id IN (7309, 7310, 7311);
UPDATE bench_t_14 SET payload = 7310 WHERE id = 14;
SELECT [bracket_7311] FROM [dbo].[tbl_31];
$dz$ dollar body 7312 ; semicolon inside $dz$
/* block header 7313 */
# hash comment 7314
-- line 7315: deterministic comment
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT nested FROM t WHERE id IN (7317, 7318, 7319);
SELECT * FROM "quoted_7318" WHERE col = E'esc\'7318';
BEGIN; SELECT 7319; COMMIT;
INSERT INTO bench_t_24 (id, payload) VALUES (7320, 'v7320');
SELECT * FROM "quoted_7321" WHERE col = E'esc\'7321';
SELECT nested FROM t WHERE id IN (7322, 7323, 7324);
# hash comment 7323
UPDATE bench_t_28 SET payload = 7324 WHERE id = 28;
/* block header 7325 */
SELECT `mysql_7326` FROM `tbl_26`;
-- line 7327: deterministic comment
BEGIN; SELECT 7328; COMMIT;
SELECT nested FROM t WHERE id IN (7329, 7330, 7331);
DELETE FROM bench_t_2 WHERE id = 2;
$dz$ dollar body 7331 ; semicolon inside $dz$
INSERT INTO bench_t_36 (id, payload) VALUES (7332, 'v7332');
SELECT `mysql_7333` FROM `tbl_33`;
SELECT [bracket_7334] FROM [dbo].[tbl_14];
INSERT INTO bench_t_39 (id, payload) VALUES (7335, 'v7335');
SELECT `mysql_7336` FROM `tbl_36`;
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT [bracket_7338] FROM [dbo].[tbl_18];
UPDATE bench_t_43 SET payload = 7339 WHERE id = 11;
SELECT `mysql_7340` FROM `tbl_40`;
WITH cte_7341 AS (SELECT 7341 AS n) SELECT n FROM cte_7341;
SELECT * FROM "quoted_7342" WHERE col = E'esc\'7342';
INSERT INTO bench_t_47 (id, payload) VALUES (7343, 'v7343');
BEGIN; SELECT 7344; COMMIT;
SELECT * FROM "quoted_7345" WHERE col = E'esc\'7345';
SELECT `mysql_7346` FROM `tbl_46`;
/* block header 7347 */
SELECT 7348 AS id, 'row_7348' AS label;
BEGIN; SELECT 7349; COMMIT;
# hash comment 7350
SELECT 7351 AS id, 'row_7351' AS label;
UPDATE bench_t_56 SET payload = 7352 WHERE id = 24;
# hash comment 7353
/* block header 7354 */
UPDATE bench_t_59 SET payload = 7355 WHERE id = 27;
SELECT [bracket_7356] FROM [dbo].[tbl_36];
-- line 7357: deterministic comment
-- line 7358: deterministic comment
WITH cte_7359 AS (SELECT 7359 AS n) SELECT n FROM cte_7359;
BEGIN; SELECT 7360; COMMIT;
-- line 7361: deterministic comment
# hash comment 7362
SELECT `mysql_7363` FROM `tbl_13`;
$dz$ dollar body 7364 ; semicolon inside $dz$
# hash comment 7365
SELECT nested FROM t WHERE id IN (7366, 7367, 7368);
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
$dz$ dollar body 7368 ; semicolon inside $dz$
BEGIN; SELECT 7369; COMMIT;
BEGIN; SELECT 7370; COMMIT;
DELETE FROM bench_t_11 WHERE id = 11;
UPDATE bench_t_12 SET payload = 7372 WHERE id = 12;
SELECT `mysql_7373` FROM `tbl_23`;
-- line 7374: deterministic comment
SELECT * FROM "quoted_7375" WHERE col = E'esc\'7375';
SELECT 7376 AS id, 'row_7376' AS label;
$dz$ dollar body 7377 ; semicolon inside $dz$
$dz$ dollar body 7378 ; semicolon inside $dz$
UPDATE bench_t_19 SET payload = 7379 WHERE id = 19;
SELECT nested FROM t WHERE id IN (7380, 7381, 7382);
INSERT INTO bench_t_85 (id, payload) VALUES (7381, 'O''Brien');
SELECT `mysql_7382` FROM `tbl_32`;
SELECT [bracket_7383] FROM [dbo].[tbl_23];
SELECT * FROM "quoted_7384" WHERE col = E'esc\'7384';
SELECT 7385 AS id, 'row_7385' AS label;
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
-- line 7387: deterministic comment
SELECT 7388 AS id, 'row_7388' AS label;
WITH cte_7389 AS (SELECT 7389 AS n) SELECT n FROM cte_7389;
$dz$ dollar body 7390 ; semicolon inside $dz$
DELETE FROM bench_t_31 WHERE id = 15;
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
DELETE FROM bench_t_1 WHERE id = 1;
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
WITH cte_7395 AS (SELECT 7395 AS n) SELECT n FROM cte_7395;
UPDATE bench_t_36 SET payload = 7396 WHERE id = 4;
SELECT [bracket_7397] FROM [dbo].[tbl_37];
SELECT 7398 AS id, 'row_7398' AS label;
BEGIN; SELECT 7399; COMMIT;
SELECT * FROM "quoted_7400" WHERE col = E'esc\'7400';
# hash comment 7401
/* block header 7402 */
/* block header 7403 */
SELECT [bracket_7404] FROM [dbo].[tbl_4];
UPDATE bench_t_45 SET payload = 7405 WHERE id = 13;
SELECT [bracket_7406] FROM [dbo].[tbl_6];
# hash comment 7407
SELECT `mysql_7408` FROM `tbl_8`;
-- line 7409: deterministic comment
SELECT * FROM "quoted_7410" WHERE col = E'esc\'7410';
SELECT [bracket_7411] FROM [dbo].[tbl_11];
SELECT * FROM "quoted_7412" WHERE col = E'esc\'7412';
# hash comment 7413
SELECT `mysql_7414` FROM `tbl_14`;
# hash comment 7415
BEGIN; SELECT 7416; COMMIT;
SELECT `mysql_7417` FROM `tbl_17`;
-- line 7418: deterministic comment
$dz$ dollar body 7419 ; semicolon inside $dz$
SELECT `mysql_7420` FROM `tbl_20`;
UPDATE bench_t_61 SET payload = 7421 WHERE id = 29;
DELETE FROM bench_t_30 WHERE id = 14;
SELECT [bracket_7423] FROM [dbo].[tbl_23];
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT [bracket_7425] FROM [dbo].[tbl_25];
$dz$ dollar body 7426 ; semicolon inside $dz$
SELECT * FROM "quoted_7427" WHERE col = E'esc\'7427';
SELECT 7428 AS id, 'row_7428' AS label;
UPDATE bench_t_5 SET payload = 7429 WHERE id = 5;
BEGIN; SELECT 7430; COMMIT;
SELECT * FROM "quoted_7431" WHERE col = E'esc\'7431';
# hash comment 7432
/* block header 7433 */
SELECT nested FROM t WHERE id IN (7434, 7435, 7436);
WITH cte_7435 AS (SELECT 7435 AS n) SELECT n FROM cte_7435;
SELECT nested FROM t WHERE id IN (7436, 7437, 7438);
-- line 7437: deterministic comment
-- line 7438: deterministic comment
SELECT [bracket_7439] FROM [dbo].[tbl_39];
BEGIN; SELECT 7440; COMMIT;
SELECT nested FROM t WHERE id IN (7441, 7442, 7443);
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
# hash comment 7444
SELECT 7445 AS id, 'row_7445' AS label;
# hash comment 7446
SELECT [bracket_7447] FROM [dbo].[tbl_7];
# hash comment 7448
SELECT nested FROM t WHERE id IN (7449, 7450, 7451);
SELECT nested FROM t WHERE id IN (7450, 7451, 7452);
BEGIN; SELECT 7451; COMMIT;
/* block header 7452 */
SELECT 7453 AS id, 'row_7453' AS label;
BEGIN; SELECT 7454; COMMIT;
SELECT `mysql_7455` FROM `tbl_5`;
-- line 7456: deterministic comment
INSERT INTO bench_t_33 (id, payload) VALUES (7457, 'v7457');
SELECT `mysql_7458` FROM `tbl_8`;
WITH cte_7459 AS (SELECT 7459 AS n) SELECT n FROM cte_7459;
SELECT nested FROM t WHERE id IN (7460, 7461, 7462);
SELECT nested FROM t WHERE id IN (7461, 7462, 7463);
DELETE FROM bench_t_6 WHERE id = 6;
INSERT INTO bench_t_39 (id, payload) VALUES (7463, 'v7463');
SELECT nested FROM t WHERE id IN (7464, 7465, 7466);
SELECT `mysql_7465` FROM `tbl_15`;
SELECT * FROM "quoted_7466" WHERE col = E'esc\'7466';
INSERT INTO bench_t_43 (id, payload) VALUES (7467, 'v7467');
SELECT [bracket_7468] FROM [dbo].[tbl_28];
SELECT [bracket_7469] FROM [dbo].[tbl_29];
UPDATE bench_t_46 SET payload = 7470 WHERE id = 14;
UPDATE bench_t_47 SET payload = 7471 WHERE id = 15;
UPDATE bench_t_48 SET payload = 7472 WHERE id = 16;
WITH cte_7473 AS (SELECT 7473 AS n) SELECT n FROM cte_7473;
SELECT `mysql_7474` FROM `tbl_24`;
SELECT `mysql_7475` FROM `tbl_25`;
SELECT 7476 AS id, 'row_7476' AS label;
-- line 7477: deterministic comment
$dz$ dollar body 7478 ; semicolon inside $dz$
UPDATE bench_t_55 SET payload = 7479 WHERE id = 23;
# hash comment 7480
BEGIN; SELECT 7481; COMMIT;
DELETE FROM bench_t_26 WHERE id = 10;
-- line 7483: deterministic comment
$dz$ dollar body 7484 ; semicolon inside $dz$
SELECT 7485 AS id, 'row_7485' AS label;
SELECT `mysql_7486` FROM `tbl_36`;
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
DELETE FROM bench_t_0 WHERE id = 0;
SELECT 7489 AS id, 'row_7489' AS label;
DELETE FROM bench_t_2 WHERE id = 2;
INSERT INTO bench_t_67 (id, payload) VALUES (7491, 'O''Brien');
SELECT [bracket_7492] FROM [dbo].[tbl_12];
WITH cte_7493 AS (SELECT 7493 AS n) SELECT n FROM cte_7493;
/* block header 7494 */
-- line 7495: deterministic comment
BEGIN; SELECT 7496; COMMIT;
-- line 7497: deterministic comment
UPDATE bench_t_10 SET payload = 7498 WHERE id = 10;
INSERT INTO bench_t_75 (id, payload) VALUES (7499, 'v7499');
/*
 * section 30
 * checksum 1800
 */
/* block header 7500 */
SELECT * FROM "quoted_7505" WHERE col = E'esc\'7505';
/* block header 7506 */
SELECT nested FROM t WHERE id IN (7507, 7508, 7509);
SELECT 7508 AS id, 'row_7508' AS label;
UPDATE bench_t_21 SET payload = 7509 WHERE id = 21;
/* block header 7510 */
SELECT * FROM "quoted_7511" WHERE col = E'esc\'7511';
SELECT nested FROM t WHERE id IN (7512, 7513, 7514);
-- line 7513: deterministic comment
$dz$ dollar body 7514 ; semicolon inside $dz$
/* block header 7515 */
# hash comment 7516
# hash comment 7517
DELETE FROM bench_t_30 WHERE id = 14;
-- line 7519: deterministic comment
/* block header 7520 */
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
$dz$ dollar body 7522 ; semicolon inside $dz$
UPDATE bench_t_35 SET payload = 7523 WHERE id = 3;
/* block header 7524 */
SELECT 7525 AS id, 'row_7525' AS label;
SELECT [bracket_7526] FROM [dbo].[tbl_6];
SELECT nested FROM t WHERE id IN (7527, 7528, 7529);
SELECT `mysql_7528` FROM `tbl_28`;
DELETE FROM bench_t_9 WHERE id = 9;
# hash comment 7530
SELECT `mysql_7531` FROM `tbl_31`;
SELECT * FROM "quoted_7532" WHERE col = E'esc\'7532';
INSERT INTO bench_t_109 (id, payload) VALUES (7533, 'v7533');
SELECT `mysql_7534` FROM `tbl_34`;
# hash comment 7535
SELECT nested FROM t WHERE id IN (7536, 7537, 7538);
UPDATE bench_t_49 SET payload = 7537 WHERE id = 17;
SELECT nested FROM t WHERE id IN (7538, 7539, 7540);
# hash comment 7539
# hash comment 7540
WITH cte_7541 AS (SELECT 7541 AS n) SELECT n FROM cte_7541;
SELECT nested FROM t WHERE id IN (7542, 7543, 7544);
# hash comment 7543
DELETE FROM bench_t_24 WHERE id = 8;
SELECT * FROM "quoted_7545" WHERE col = E'esc\'7545';
UPDATE bench_t_58 SET payload = 7546 WHERE id = 26;
/* block header 7547 */
DELETE FROM bench_t_28 WHERE id = 12;
SELECT 7549 AS id, 'row_7549' AS label;
SELECT `mysql_7550` FROM `tbl_0`;
BEGIN; SELECT 7551; COMMIT;
/* block header 7552 */
INSERT INTO bench_t_1 (id, payload) VALUES (7553, 'v7553');
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
-- line 7555: deterministic comment
SELECT `mysql_7556` FROM `tbl_6`;
UPDATE bench_t_5 SET payload = 7557 WHERE id = 5;
BEGIN; SELECT 7558; COMMIT;
$dz$ dollar body 7559 ; semicolon inside $dz$
UPDATE bench_t_8 SET payload = 7560 WHERE id = 8;
SELECT 7561 AS id, 'row_7561' AS label;
SELECT * FROM "quoted_7562" WHERE col = E'esc\'7562';
/* block header 7563 */
/* block header 7564 */
SELECT * FROM "quoted_7565" WHERE col = E'esc\'7565';
BEGIN; SELECT 7566; COMMIT;
/* block header 7567 */
# hash comment 7568
SELECT * FROM "quoted_7569" WHERE col = E'esc\'7569';
WITH cte_7570 AS (SELECT 7570 AS n) SELECT n FROM cte_7570;
SELECT nested FROM t WHERE id IN (7571, 7572, 7573);
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT nested FROM t WHERE id IN (7573, 7574, 7575);
$dz$ dollar body 7574 ; semicolon inside $dz$
/* block header 7575 */
DELETE FROM bench_t_24 WHERE id = 8;
/* block header 7577 */
# hash comment 7578
UPDATE bench_t_27 SET payload = 7579 WHERE id = 27;
-- line 7580: deterministic comment
INSERT INTO bench_t_29 (id, payload) VALUES (7581, 'v7581');
DELETE FROM bench_t_30 WHERE id = 14;
SELECT * FROM "quoted_7583" WHERE col = E'esc\'7583';
# hash comment 7584
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
UPDATE bench_t_34 SET payload = 7586 WHERE id = 2;
SELECT * FROM "quoted_7587" WHERE col = E'esc\'7587';
INSERT INTO bench_t_36 (id, payload) VALUES (7588, 'v7588');
SELECT * FROM "quoted_7589" WHERE col = E'esc\'7589';
SELECT * FROM "quoted_7590" WHERE col = E'esc\'7590';
SELECT `mysql_7591` FROM `tbl_41`;
$dz$ dollar body 7592 ; semicolon inside $dz$
SELECT `mysql_7593` FROM `tbl_43`;
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT nested FROM t WHERE id IN (7595, 7596, 7597);
UPDATE bench_t_44 SET payload = 7596 WHERE id = 12;
SELECT * FROM "quoted_7597" WHERE col = E'esc\'7597';
BEGIN; SELECT 7598; COMMIT;
-- line 7599: deterministic comment
# hash comment 7600
$dz$ dollar body 7601 ; semicolon inside $dz$
INSERT INTO bench_t_50 (id, payload) VALUES (7602, 'v7602');
$dz$ dollar body 7603 ; semicolon inside $dz$
BEGIN; SELECT 7604; COMMIT;
SELECT 7605 AS id, 'row_7605' AS label;
-- line 7606: deterministic comment
SELECT `mysql_7607` FROM `tbl_7`;
SELECT `mysql_7608` FROM `tbl_8`;
$dz$ dollar body 7609 ; semicolon inside $dz$
SELECT nested FROM t WHERE id IN (7610, 7611, 7612);
SELECT * FROM "quoted_7611" WHERE col = E'esc\'7611';
-- line 7612: deterministic comment
$dz$ dollar body 7613 ; semicolon inside $dz$
/* block header 7614 */
SELECT nested FROM t WHERE id IN (7615, 7616, 7617);
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT nested FROM t WHERE id IN (7617, 7618, 7619);
-- line 7618: deterministic comment
SELECT 7619 AS id, 'row_7619' AS label;
# hash comment 7620
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT 7623 AS id, 'row_7623' AS label;
SELECT * FROM "quoted_7624" WHERE col = E'esc\'7624';
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
WITH cte_7627 AS (SELECT 7627 AS n) SELECT n FROM cte_7627;
SELECT [bracket_7628] FROM [dbo].[tbl_28];
BEGIN; SELECT 7629; COMMIT;
SELECT 7630 AS id, 'row_7630' AS label;
SELECT nested FROM t WHERE id IN (7631, 7632, 7633);
UPDATE bench_t_16 SET payload = 7632 WHERE id = 16;
SELECT [bracket_7633] FROM [dbo].[tbl_33];
WITH cte_7634 AS (SELECT 7634 AS n) SELECT n FROM cte_7634;
INSERT INTO bench_t_83 (id, payload) VALUES (7635, 'v7635');
SELECT [bracket_7636] FROM [dbo].[tbl_36];
INSERT INTO bench_t_85 (id, payload) VALUES (7637, 'v7637');
SELECT `mysql_7638` FROM `tbl_38`;
$dz$ dollar body 7639 ; semicolon inside $dz$
SELECT [bracket_7640] FROM [dbo].[tbl_0];
DELETE FROM bench_t_25 WHERE id = 9;
INSERT INTO bench_t_90 (id, payload) VALUES (7642, 'v7642');
SELECT * FROM "quoted_7643" WHERE col = E'esc\'7643';
SELECT 7644 AS id, 'row_7644' AS label;
BEGIN; SELECT 7645; COMMIT;
UPDATE bench_t_30 SET payload = 7646 WHERE id = 30;
DELETE FROM bench_t_31 WHERE id = 15;
SELECT * FROM "quoted_7648" WHERE col = E'esc\'7648';
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
INSERT INTO bench_t_98 (id, payload) VALUES (7650, 'v7650');
SELECT * FROM "quoted_7651" WHERE col = E'esc\'7651';
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT `mysql_7653` FROM `tbl_3`;
SELECT [bracket_7654] FROM [dbo].[tbl_14];
-- line 7655: deterministic comment
BEGIN; SELECT 7656; COMMIT;
INSERT INTO bench_t_105 (id, payload) VALUES (7657, 'v7657');
DELETE FROM bench_t_10 WHERE id = 10;
BEGIN; SELECT 7659; COMMIT;
INSERT INTO bench_t_108 (id, payload) VALUES (7660, 'v7660');
INSERT INTO bench_t_109 (id, payload) VALUES (7661, 'v7661');
/* block header 7662 */
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
UPDATE bench_t_48 SET payload = 7664 WHERE id = 16;
DELETE FROM bench_t_17 WHERE id = 1;
BEGIN; SELECT 7666; COMMIT;
SELECT [bracket_7667] FROM [dbo].[tbl_27];
SELECT nested FROM t WHERE id IN (7668, 7669, 7670);
SELECT 7669 AS id, 'row_7669' AS label;
INSERT INTO bench_t_118 (id, payload) VALUES (7670, 'v7670');
DELETE FROM bench_t_23 WHERE id = 7;
WITH cte_7672 AS (SELECT 7672 AS n) SELECT n FROM cte_7672;
BEGIN; SELECT 7673; COMMIT;
SELECT `mysql_7674` FROM `tbl_24`;
SELECT * FROM "quoted_7675" WHERE col = E'esc\'7675';
$dz$ dollar body 7676 ; semicolon inside $dz$
DELETE FROM bench_t_29 WHERE id = 13;
SELECT `mysql_7678` FROM `tbl_28`;
SELECT 7679 AS id, 'row_7679' AS label;
INSERT INTO bench_t_0 (id, payload) VALUES (7680, 'v7680');
-- line 7681: deterministic comment
SELECT [bracket_7682] FROM [dbo].[tbl_2];
DELETE FROM bench_t_3 WHERE id = 3;
$dz$ dollar body 7684 ; semicolon inside $dz$
SELECT [bracket_7685] FROM [dbo].[tbl_5];
INSERT INTO bench_t_6 (id, payload) VALUES (7686, 'v7686');
SELECT nested FROM t WHERE id IN (7687, 7688, 7689);
SELECT nested FROM t WHERE id IN (7688, 7689, 7690);
INSERT INTO bench_t_9 (id, payload) VALUES (7689, 'O''Brien');
SELECT `mysql_7690` FROM `tbl_40`;
INSERT INTO bench_t_11 (id, payload) VALUES (7691, 'v7691');
DELETE FROM bench_t_12 WHERE id = 12;
WITH cte_7693 AS (SELECT 7693 AS n) SELECT n FROM cte_7693;
UPDATE bench_t_14 SET payload = 7694 WHERE id = 14;
BEGIN; SELECT 7695; COMMIT;
SELECT nested FROM t WHERE id IN (7696, 7697, 7698);
WITH cte_7697 AS (SELECT 7697 AS n) SELECT n FROM cte_7697;
INSERT INTO bench_t_18 (id, payload) VALUES (7698, 'v7698');
DELETE FROM bench_t_19 WHERE id = 3;
SELECT * FROM "quoted_7700" WHERE col = E'esc\'7700';
DELETE FROM bench_t_21 WHERE id = 5;
INSERT INTO bench_t_22 (id, payload) VALUES (7702, 'v7702');
SELECT [bracket_7703] FROM [dbo].[tbl_23];
SELECT [bracket_7704] FROM [dbo].[tbl_24];
SELECT 7705 AS id, 'row_7705' AS label;
SELECT * FROM "quoted_7706" WHERE col = E'esc\'7706';
SELECT nested FROM t WHERE id IN (7707, 7708, 7709);
WITH cte_7708 AS (SELECT 7708 AS n) SELECT n FROM cte_7708;
INSERT INTO bench_t_29 (id, payload) VALUES (7709, 'v7709');
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
BEGIN; SELECT 7711; COMMIT;
SELECT * FROM "quoted_7712" WHERE col = E'esc\'7712';
SELECT nested FROM t WHERE id IN (7713, 7714, 7715);
DELETE FROM bench_t_2 WHERE id = 2;
$dz$ dollar body 7715 ; semicolon inside $dz$
INSERT INTO bench_t_36 (id, payload) VALUES (7716, 'v7716');
WITH cte_7717 AS (SELECT 7717 AS n) SELECT n FROM cte_7717;
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
/* block header 7719 */
INSERT INTO bench_t_40 (id, payload) VALUES (7720, 'v7720');
INSERT INTO bench_t_41 (id, payload) VALUES (7721, 'v7721');
SELECT 7722 AS id, 'row_7722' AS label;
SELECT 7723 AS id, 'row_7723' AS label;
SELECT `mysql_7724` FROM `tbl_24`;
/* block header 7725 */
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
INSERT INTO bench_t_47 (id, payload) VALUES (7727, 'v7727');
INSERT INTO bench_t_48 (id, payload) VALUES (7728, 'v7728');
SELECT nested FROM t WHERE id IN (7729, 7730, 7731);
INSERT INTO bench_t_50 (id, payload) VALUES (7730, 'v7730');
# hash comment 7731
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
DELETE FROM bench_t_21 WHERE id = 5;
INSERT INTO bench_t_54 (id, payload) VALUES (7734, 'v7734');
SELECT nested FROM t WHERE id IN (7735, 7736, 7737);
/* block header 7736 */
SELECT `mysql_7737` FROM `tbl_37`;
WITH cte_7738 AS (SELECT 7738 AS n) SELECT n FROM cte_7738;
INSERT INTO bench_t_59 (id, payload) VALUES (7739, 'v7739');
BEGIN; SELECT 7740; COMMIT;
INSERT INTO bench_t_61 (id, payload) VALUES (7741, 'v7741');
UPDATE bench_t_62 SET payload = 7742 WHERE id = 30;
UPDATE bench_t_63 SET payload = 7743 WHERE id = 31;
INSERT INTO bench_t_64 (id, payload) VALUES (7744, 'O''Brien');
INSERT INTO bench_t_65 (id, payload) VALUES (7745, 'v7745');
SELECT * FROM "quoted_7746" WHERE col = E'esc\'7746';
WITH cte_7747 AS (SELECT 7747 AS n) SELECT n FROM cte_7747;
SELECT * FROM "quoted_7748" WHERE col = E'esc\'7748';
SELECT `mysql_7749` FROM `tbl_49`;
/*
 * section 31
 * checksum 9a96
 */
-- line 7750: deterministic comment
SELECT * FROM "quoted_7755" WHERE col = E'esc\'7755';
SELECT nested FROM t WHERE id IN (7756, 7757, 7758);
/* block header 7757 */
SELECT `mysql_7758` FROM `tbl_8`;
-- line 7759: deterministic comment
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
INSERT INTO bench_t_81 (id, payload) VALUES (7761, 'v7761');
SELECT nested FROM t WHERE id IN (7762, 7763, 7764);
BEGIN; SELECT 7763; COMMIT;
# hash comment 7764
DELETE FROM bench_t_21 WHERE id = 5;
-- line 7766: deterministic comment
$dz$ dollar body 7767 ; semicolon inside $dz$
SELECT `mysql_7768` FROM `tbl_18`;
WITH cte_7769 AS (SELECT 7769 AS n) SELECT n FROM cte_7769;
-- line 7770: deterministic comment
BEGIN; SELECT 7771; COMMIT;
-- line 7772: deterministic comment
-- line 7773: deterministic comment
DELETE FROM bench_t_30 WHERE id = 14;
# hash comment 7775
SELECT * FROM "quoted_7776" WHERE col = E'esc\'7776';
$dz$ dollar body 7777 ; semicolon inside $dz$
# hash comment 7778
$dz$ dollar body 7779 ; semicolon inside $dz$
SELECT `mysql_7780` FROM `tbl_30`;
-- line 7781: deterministic comment
$dz$ dollar body 7782 ; semicolon inside $dz$
DELETE FROM bench_t_7 WHERE id = 7;
SELECT `mysql_7784` FROM `tbl_34`;
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
WITH cte_7786 AS (SELECT 7786 AS n) SELECT n FROM cte_7786;
DELETE FROM bench_t_11 WHERE id = 11;
WITH cte_7788 AS (SELECT 7788 AS n) SELECT n FROM cte_7788;
WITH cte_7789 AS (SELECT 7789 AS n) SELECT n FROM cte_7789;
$dz$ dollar body 7790 ; semicolon inside $dz$
WITH cte_7791 AS (SELECT 7791 AS n) SELECT n FROM cte_7791;
BEGIN; SELECT 7792; COMMIT;
SELECT [bracket_7793] FROM [dbo].[tbl_33];
WITH cte_7794 AS (SELECT 7794 AS n) SELECT n FROM cte_7794;
BEGIN; SELECT 7795; COMMIT;
-- line 7796: deterministic comment
DELETE FROM bench_t_21 WHERE id = 5;
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT nested FROM t WHERE id IN (7799, 7800, 7801);
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT nested FROM t WHERE id IN (7801, 7802, 7803);
WITH cte_7802 AS (SELECT 7802 AS n) SELECT n FROM cte_7802;
$dz$ dollar body 7803 ; semicolon inside $dz$
$dz$ dollar body 7804 ; semicolon inside $dz$
INSERT INTO bench_t_125 (id, payload) VALUES (7805, 'v7805');
/* block header 7806 */
INSERT INTO bench_t_127 (id, payload) VALUES (7807, 'v7807');
/* block header 7808 */
SELECT `mysql_7809` FROM `tbl_9`;
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
$dz$ dollar body 7811 ; semicolon inside $dz$
$dz$ dollar body 7812 ; semicolon inside $dz$
BEGIN; SELECT 7813; COMMIT;
UPDATE bench_t_6 SET payload = 7814 WHERE id = 6;
INSERT INTO bench_t_7 (id, payload) VALUES (7815, 'v7815');
SELECT nested FROM t WHERE id IN (7816, 7817, 7818);
INSERT INTO bench_t_9 (id, payload) VALUES (7817, 'v7817');
# hash comment 7818
UPDATE bench_t_11 SET payload = 7819 WHERE id = 11;
DELETE FROM bench_t_12 WHERE id = 12;
WITH cte_7821 AS (SELECT 7821 AS n) SELECT n FROM cte_7821;
WITH cte_7822 AS (SELECT 7822 AS n) SELECT n FROM cte_7822;
/* block header 7823 */
SELECT [bracket_7824] FROM [dbo].[tbl_24];
BEGIN; SELECT 7825; COMMIT;
WITH cte_7826 AS (SELECT 7826 AS n) SELECT n FROM cte_7826;
# hash comment 7827
DELETE FROM bench_t_20 WHERE id = 4;
SELECT [bracket_7829] FROM [dbo].[tbl_29];
DELETE FROM bench_t_22 WHERE id = 6;
$dz$ dollar body 7831 ; semicolon inside $dz$
/* block header 7832 */
BEGIN; SELECT 7833; COMMIT;
SELECT * FROM "quoted_7834" WHERE col = E'esc\'7834';
SELECT * FROM "quoted_7835" WHERE col = E'esc\'7835';
INSERT INTO bench_t_28 (id, payload) VALUES (7836, 'v7836');
-- line 7837: deterministic comment
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
# hash comment 7839
/* block header 7840 */
BEGIN; SELECT 7841; COMMIT;
$dz$ dollar body 7842 ; semicolon inside $dz$
-- line 7843: deterministic comment
INSERT INTO bench_t_36 (id, payload) VALUES (7844, 'v7844');
$dz$ dollar body 7845 ; semicolon inside $dz$
/* block header 7846 */
UPDATE bench_t_39 SET payload = 7847 WHERE id = 7;
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
WITH cte_7849 AS (SELECT 7849 AS n) SELECT n FROM cte_7849;
BEGIN; SELECT 7850; COMMIT;
# hash comment 7851
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
/* block header 7853 */
UPDATE bench_t_46 SET payload = 7854 WHERE id = 14;
$dz$ dollar body 7855 ; semicolon inside $dz$
$dz$ dollar body 7856 ; semicolon inside $dz$
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
$dz$ dollar body 7858 ; semicolon inside $dz$
/* block header 7859 */
SELECT [bracket_7860] FROM [dbo].[tbl_20];
# hash comment 7861
BEGIN; SELECT 7862; COMMIT;
SELECT nested FROM t WHERE id IN (7863, 7864, 7865);
DELETE FROM bench_t_24 WHERE id = 8;
SELECT [bracket_7865] FROM [dbo].[tbl_25];
# hash comment 7866
SELECT nested FROM t WHERE id IN (7867, 7868, 7869);
SELECT nested FROM t WHERE id IN (7868, 7869, 7870);
INSERT INTO bench_t_61 (id, payload) VALUES (7869, 'v7869');
SELECT [bracket_7870] FROM [dbo].[tbl_30];
# hash comment 7871
SELECT * FROM "quoted_7872" WHERE col = E'esc\'7872';
DELETE FROM bench_t_1 WHERE id = 1;
WITH cte_7874 AS (SELECT 7874 AS n) SELECT n FROM cte_7874;
SELECT [bracket_7875] FROM [dbo].[tbl_35];
DELETE FROM bench_t_4 WHERE id = 4;
# hash comment 7877
SELECT [bracket_7878] FROM [dbo].[tbl_38];
SELECT 7879 AS id, 'row_7879' AS label;
SELECT 7880 AS id, 'row_7880' AS label;
SELECT 7881 AS id, 'row_7881' AS label;
UPDATE bench_t_10 SET payload = 7882 WHERE id = 10;
BEGIN; SELECT 7883; COMMIT;
$dz$ dollar body 7884 ; semicolon inside $dz$
INSERT INTO bench_t_77 (id, payload) VALUES (7885, 'v7885');
SELECT 7886 AS id, 'row_7886' AS label;
# hash comment 7887
SELECT [bracket_7888] FROM [dbo].[tbl_8];
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT [bracket_7890] FROM [dbo].[tbl_10];
UPDATE bench_t_19 SET payload = 7891 WHERE id = 19;
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT `mysql_7893` FROM `tbl_43`;
SELECT * FROM "quoted_7894" WHERE col = E'esc\'7894';
SELECT 7895 AS id, 'row_7895' AS label;
SELECT 7896 AS id, 'row_7896' AS label;
DELETE FROM bench_t_25 WHERE id = 9;
UPDATE bench_t_26 SET payload = 7898 WHERE id = 26;
BEGIN; SELECT 7899; COMMIT;
# hash comment 7900
SELECT [bracket_7901] FROM [dbo].[tbl_21];
INSERT INTO bench_t_94 (id, payload) VALUES (7902, 'v7902');
SELECT 7903 AS id, 'row_7903' AS label;
SELECT * FROM "quoted_7904" WHERE col = E'esc\'7904';
/* block header 7905 */
SELECT * FROM "quoted_7906" WHERE col = E'esc\'7906';
# hash comment 7907
SELECT nested FROM t WHERE id IN (7908, 7909, 7910);
SELECT nested FROM t WHERE id IN (7909, 7910, 7911);
DELETE FROM bench_t_6 WHERE id = 6;
INSERT INTO bench_t_103 (id, payload) VALUES (7911, 'v7911');
$dz$ dollar body 7912 ; semicolon inside $dz$
-- line 7913: deterministic comment
/* block header 7914 */
-- line 7915: deterministic comment
BEGIN; SELECT 7916; COMMIT;
SELECT nested FROM t WHERE id IN (7917, 7918, 7919);
SELECT 7918 AS id, 'row_7918' AS label;
UPDATE bench_t_47 SET payload = 7919 WHERE id = 15;
INSERT INTO bench_t_112 (id, payload) VALUES (7920, 'O''Brien');
# hash comment 7921
SELECT nested FROM t WHERE id IN (7922, 7923, 7924);
SELECT [bracket_7923] FROM [dbo].[tbl_3];
WITH cte_7924 AS (SELECT 7924 AS n) SELECT n FROM cte_7924;
SELECT * FROM "quoted_7925" WHERE col = E'esc\'7925';
UPDATE bench_t_54 SET payload = 7926 WHERE id = 22;
SELECT [bracket_7927] FROM [dbo].[tbl_7];
$dz$ dollar body 7928 ; semicolon inside $dz$
SELECT [bracket_7929] FROM [dbo].[tbl_9];
SELECT * FROM "quoted_7930" WHERE col = E'esc\'7930';
-- line 7931: deterministic comment
/* block header 7932 */
BEGIN; SELECT 7933; COMMIT;
SELECT * FROM "quoted_7934" WHERE col = E'esc\'7934';
INSERT INTO bench_t_127 (id, payload) VALUES (7935, 'v7935');
WITH cte_7936 AS (SELECT 7936 AS n) SELECT n FROM cte_7936;
INSERT INTO bench_t_1 (id, payload) VALUES (7937, 'v7937');
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
DELETE FROM bench_t_3 WHERE id = 3;
-- line 7940: deterministic comment
# hash comment 7941
INSERT INTO bench_t_6 (id, payload) VALUES (7942, 'O''Brien');
SELECT * FROM "quoted_7943" WHERE col = E'esc\'7943';
SELECT nested FROM t WHERE id IN (7944, 7945, 7946);
-- line 7945: deterministic comment
WITH cte_7946 AS (SELECT 7946 AS n) SELECT n FROM cte_7946;
BEGIN; SELECT 7947; COMMIT;
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
/* block header 7949 */
DELETE FROM bench_t_14 WHERE id = 14;
/* block header 7951 */
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
/* block header 7953 */
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
INSERT INTO bench_t_19 (id, payload) VALUES (7955, 'v7955');
# hash comment 7956
SELECT 7957 AS id, 'row_7957' AS label;
# hash comment 7958
BEGIN; SELECT 7959; COMMIT;
UPDATE bench_t_24 SET payload = 7960 WHERE id = 24;
DELETE FROM bench_t_25 WHERE id = 9;
SELECT `mysql_7962` FROM `tbl_12`;
# hash comment 7963
UPDATE bench_t_28 SET payload = 7964 WHERE id = 28;
INSERT INTO bench_t_29 (id, payload) VALUES (7965, 'v7965');
/* block header 7966 */
SELECT * FROM "quoted_7967" WHERE col = E'esc\'7967';
UPDATE bench_t_32 SET payload = 7968 WHERE id = 0;
WITH cte_7969 AS (SELECT 7969 AS n) SELECT n FROM cte_7969;
UPDATE bench_t_34 SET payload = 7970 WHERE id = 2;
SELECT [bracket_7971] FROM [dbo].[tbl_11];
$dz$ dollar body 7972 ; semicolon inside $dz$
SELECT nested FROM t WHERE id IN (7973, 7974, 7975);
SELECT `mysql_7974` FROM `tbl_24`;
WITH cte_7975 AS (SELECT 7975 AS n) SELECT n FROM cte_7975;
UPDATE bench_t_40 SET payload = 7976 WHERE id = 8;
DELETE FROM bench_t_9 WHERE id = 9;
-- line 7978: deterministic comment
# hash comment 7979
SELECT 7980 AS id, 'row_7980' AS label;
UPDATE bench_t_45 SET payload = 7981 WHERE id = 13;
INSERT INTO bench_t_46 (id, payload) VALUES (7982, 'v7982');
SELECT 7983 AS id, 'row_7983' AS label;
SELECT * FROM "quoted_7984" WHERE col = E'esc\'7984';
DELETE FROM bench_t_17 WHERE id = 1;
SELECT `mysql_7986` FROM `tbl_36`;
UPDATE bench_t_51 SET payload = 7987 WHERE id = 19;
BEGIN; SELECT 7988; COMMIT;
UPDATE bench_t_53 SET payload = 7989 WHERE id = 21;
BEGIN; SELECT 7990; COMMIT;
-- line 7991: deterministic comment
/* block header 7992 */
$dz$ dollar body 7993 ; semicolon inside $dz$
SELECT nested FROM t WHERE id IN (7994, 7995, 7996);
SELECT * FROM "quoted_7995" WHERE col = E'esc\'7995';
SELECT [bracket_7996] FROM [dbo].[tbl_36];
SELECT 7997 AS id, 'row_7997' AS label;
$dz$ dollar body 7998 ; semicolon inside $dz$
SELECT * FROM "quoted_7999" WHERE col = E'esc\'7999';
/*
 * section 32
 * checksum f9f5
 */
$dz$ dollar body 8000 ; semicolon inside $dz$
SELECT nested FROM t WHERE id IN (8005, 8006, 8007);
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
INSERT INTO bench_t_71 (id, payload) VALUES (8007, 'v8007');
SELECT 8008 AS id, 'row_8008' AS label;
/* block header 8009 */
BEGIN; SELECT 8010; COMMIT;
/* block header 8011 */
SELECT nested FROM t WHERE id IN (8012, 8013, 8014);
INSERT INTO bench_t_77 (id, payload) VALUES (8013, 'v8013');
UPDATE bench_t_14 SET payload = 8014 WHERE id = 14;
/* block header 8015 */
INSERT INTO bench_t_80 (id, payload) VALUES (8016, 'v8016');
INSERT INTO bench_t_81 (id, payload) VALUES (8017, 'v8017');
SELECT `mysql_8018` FROM `tbl_18`;
SELECT 8019 AS id, 'row_8019' AS label;
-- line 8020: deterministic comment
SELECT 8021 AS id, 'row_8021' AS label;
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
$dz$ dollar body 8023 ; semicolon inside $dz$
SELECT [bracket_8024] FROM [dbo].[tbl_24];
UPDATE bench_t_25 SET payload = 8025 WHERE id = 25;
SELECT [bracket_8026] FROM [dbo].[tbl_26];
SELECT 8027 AS id, 'row_8027' AS label;
SELECT nested FROM t WHERE id IN (8028, 8029, 8030);
-- line 8029: deterministic comment
SELECT * FROM "quoted_8030" WHERE col = E'esc\'8030';
DELETE FROM bench_t_31 WHERE id = 15;
SELECT [bracket_8032] FROM [dbo].[tbl_32];
INSERT INTO bench_t_97 (id, payload) VALUES (8033, 'v8033');
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
UPDATE bench_t_35 SET payload = 8035 WHERE id = 3;
SELECT [bracket_8036] FROM [dbo].[tbl_36];
SELECT nested FROM t WHERE id IN (8037, 8038, 8039);
-- line 8038: deterministic comment
# hash comment 8039
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT 8041 AS id, 'row_8041' AS label;
SELECT `mysql_8042` FROM `tbl_42`;
SELECT * FROM "quoted_8043" WHERE col = E'esc\'8043';
SELECT `mysql_8044` FROM `tbl_44`;
DELETE FROM bench_t_13 WHERE id = 13;
UPDATE bench_t_46 SET payload = 8046 WHERE id = 14;
INSERT INTO bench_t_111 (id, payload) VALUES (8047, 'v8047');
SELECT 8048 AS id, 'row_8048' AS label;
/* block header 8049 */
SELECT * FROM "quoted_8050" WHERE col = E'esc\'8050';
# hash comment 8051
INSERT INTO bench_t_116 (id, payload) VALUES (8052, 'O''Brien');
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
$dz$ dollar body 8054 ; semicolon inside $dz$
SELECT nested FROM t WHERE id IN (8055, 8056, 8057);
WITH cte_8056 AS (SELECT 8056 AS n) SELECT n FROM cte_8056;
INSERT INTO bench_t_121 (id, payload) VALUES (8057, 'v8057');
SELECT [bracket_8058] FROM [dbo].[tbl_18];
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
$dz$ dollar body 8060 ; semicolon inside $dz$
SELECT 8061 AS id, 'row_8061' AS label;
WITH cte_8062 AS (SELECT 8062 AS n) SELECT n FROM cte_8062;
UPDATE bench_t_63 SET payload = 8063 WHERE id = 31;
SELECT * FROM "quoted_8064" WHERE col = E'esc\'8064';
SELECT [bracket_8065] FROM [dbo].[tbl_25];
SELECT `mysql_8066` FROM `tbl_16`;
UPDATE bench_t_3 SET payload = 8067 WHERE id = 3;
SELECT * FROM "quoted_8068" WHERE col = E'esc\'8068';
SELECT 8069 AS id, 'row_8069' AS label;
SELECT [bracket_8070] FROM [dbo].[tbl_30];
SELECT 8071 AS id, 'row_8071' AS label;
BEGIN; SELECT 8072; COMMIT;
WITH cte_8073 AS (SELECT 8073 AS n) SELECT n FROM cte_8073;
SELECT `mysql_8074` FROM `tbl_24`;
SELECT 8075 AS id, 'row_8075' AS label;
$dz$ dollar body 8076 ; semicolon inside $dz$
SELECT `mysql_8077` FROM `tbl_27`;
SELECT 8078 AS id, 'row_8078' AS label;
SELECT [bracket_8079] FROM [dbo].[tbl_39];
/* block header 8080 */
SELECT nested FROM t WHERE id IN (8081, 8082, 8083);
$dz$ dollar body 8082 ; semicolon inside $dz$
-- line 8083: deterministic comment
UPDATE bench_t_20 SET payload = 8084 WHERE id = 20;
SELECT * FROM "quoted_8085" WHERE col = E'esc\'8085';
SELECT `mysql_8086` FROM `tbl_36`;
SELECT * FROM "quoted_8087" WHERE col = E'esc\'8087';
INSERT INTO bench_t_24 (id, payload) VALUES (8088, 'v8088');
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
INSERT INTO bench_t_26 (id, payload) VALUES (8090, 'v8090');
SELECT nested FROM t WHERE id IN (8091, 8092, 8093);
-- line 8092: deterministic comment
UPDATE bench_t_29 SET payload = 8093 WHERE id = 29;
SELECT `mysql_8094` FROM `tbl_44`;
BEGIN; SELECT 8095; COMMIT;
$dz$ dollar body 8096 ; semicolon inside $dz$
DELETE FROM bench_t_1 WHERE id = 1;
DELETE FROM bench_t_2 WHERE id = 2;
BEGIN; SELECT 8099; COMMIT;
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
# hash comment 8101
INSERT INTO bench_t_38 (id, payload) VALUES (8102, 'v8102');
SELECT 8103 AS id, 'row_8103' AS label;
SELECT 8104 AS id, 'row_8104' AS label;
-- line 8105: deterministic comment
SELECT [bracket_8106] FROM [dbo].[tbl_26];
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
$dz$ dollar body 8108 ; semicolon inside $dz$
SELECT * FROM "quoted_8109" WHERE col = E'esc\'8109';
$dz$ dollar body 8110 ; semicolon inside $dz$
SELECT [bracket_8111] FROM [dbo].[tbl_31];
SELECT * FROM "quoted_8112" WHERE col = E'esc\'8112';
$dz$ dollar body 8113 ; semicolon inside $dz$
UPDATE bench_t_50 SET payload = 8114 WHERE id = 18;
SELECT `mysql_8115` FROM `tbl_15`;
WITH cte_8116 AS (SELECT 8116 AS n) SELECT n FROM cte_8116;
INSERT INTO bench_t_53 (id, payload) VALUES (8117, 'v8117');
/* block header 8118 */
SELECT `mysql_8119` FROM `tbl_19`;
SELECT 8120 AS id, 'row_8120' AS label;
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
# hash comment 8122
WITH cte_8123 AS (SELECT 8123 AS n) SELECT n FROM cte_8123;
SELECT nested FROM t WHERE id IN (8124, 8125, 8126);
# hash comment 8125
$dz$ dollar body 8126 ; semicolon inside $dz$
-- line 8127: deterministic comment
SELECT 8128 AS id, 'row_8128' AS label;
$dz$ dollar body 8129 ; semicolon inside $dz$
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
BEGIN; SELECT 8131; COMMIT;
SELECT nested FROM t WHERE id IN (8132, 8133, 8134);
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
BEGIN; SELECT 8134; COMMIT;
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT 8136 AS id, 'row_8136' AS label;
SELECT 8137 AS id, 'row_8137' AS label;
UPDATE bench_t_10 SET payload = 8138 WHERE id = 10;
SELECT 8139 AS id, 'row_8139' AS label;
SELECT `mysql_8140` FROM `tbl_40`;
/* block header 8141 */
# hash comment 8142
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
/* block header 8144 */
$dz$ dollar body 8145 ; semicolon inside $dz$
SELECT * FROM "quoted_8146" WHERE col = E'esc\'8146';
/* block header 8147 */
UPDATE bench_t_20 SET payload = 8148 WHERE id = 20;
BEGIN; SELECT 8149; COMMIT;
SELECT 8150 AS id, 'row_8150' AS label;
SELECT nested FROM t WHERE id IN (8151, 8152, 8153);
# hash comment 8152
$dz$ dollar body 8153 ; semicolon inside $dz$
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT `mysql_8155` FROM `tbl_5`;
BEGIN; SELECT 8156; COMMIT;
# hash comment 8157
SELECT 8158 AS id, 'row_8158' AS label;
UPDATE bench_t_31 SET payload = 8159 WHERE id = 31;
$dz$ dollar body 8160 ; semicolon inside $dz$
$dz$ dollar body 8161 ; semicolon inside $dz$
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
UPDATE bench_t_35 SET payload = 8163 WHERE id = 3;
$dz$ dollar body 8164 ; semicolon inside $dz$
SELECT 8165 AS id, 'row_8165' AS label;
SELECT * FROM "quoted_8166" WHERE col = E'esc\'8166';
UPDATE bench_t_39 SET payload = 8167 WHERE id = 7;
SELECT `mysql_8168` FROM `tbl_18`;
DELETE FROM bench_t_9 WHERE id = 9;
UPDATE bench_t_42 SET payload = 8170 WHERE id = 10;
SELECT * FROM "quoted_8171" WHERE col = E'esc\'8171';
SELECT 8172 AS id, 'row_8172' AS label;
SELECT `mysql_8173` FROM `tbl_23`;
DELETE FROM bench_t_14 WHERE id = 14;
$dz$ dollar body 8175 ; semicolon inside $dz$
SELECT `mysql_8176` FROM `tbl_26`;
SELECT [bracket_8177] FROM [dbo].[tbl_17];
$dz$ dollar body 8178 ; semicolon inside $dz$
SELECT `mysql_8179` FROM `tbl_29`;
SELECT 8180 AS id, 'row_8180' AS label;
WITH cte_8181 AS (SELECT 8181 AS n) SELECT n FROM cte_8181;
WITH cte_8182 AS (SELECT 8182 AS n) SELECT n FROM cte_8182;
BEGIN; SELECT 8183; COMMIT;
SELECT * FROM "quoted_8184" WHERE col = E'esc\'8184';
SELECT * FROM "quoted_8185" WHERE col = E'esc\'8185';
$dz$ dollar body 8186 ; semicolon inside $dz$
INSERT INTO bench_t_123 (id, payload) VALUES (8187, 'v8187');
WITH cte_8188 AS (SELECT 8188 AS n) SELECT n FROM cte_8188;
DELETE FROM bench_t_29 WHERE id = 13;
DELETE FROM bench_t_30 WHERE id = 14;
SELECT [bracket_8191] FROM [dbo].[tbl_31];
UPDATE bench_t_0 SET payload = 8192 WHERE id = 0;
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT nested FROM t WHERE id IN (8194, 8195, 8196);
INSERT INTO bench_t_3 (id, payload) VALUES (8195, 'O''Brien');
SELECT nested FROM t WHERE id IN (8196, 8197, 8198);
SELECT `mysql_8197` FROM `tbl_47`;
INSERT INTO bench_t_6 (id, payload) VALUES (8198, 'v8198');
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
UPDATE bench_t_8 SET payload = 8200 WHERE id = 8;
SELECT 8201 AS id, 'row_8201' AS label;
$dz$ dollar body 8202 ; semicolon inside $dz$
BEGIN; SELECT 8203; COMMIT;
-- line 8204: deterministic comment
UPDATE bench_t_13 SET payload = 8205 WHERE id = 13;
SELECT 8206 AS id, 'row_8206' AS label;
SELECT nested FROM t WHERE id IN (8207, 8208, 8209);
WITH cte_8208 AS (SELECT 8208 AS n) SELECT n FROM cte_8208;
-- line 8209: deterministic comment
SELECT 8210 AS id, 'row_8210' AS label;
UPDATE bench_t_19 SET payload = 8211 WHERE id = 19;
SELECT `mysql_8212` FROM `tbl_12`;
SELECT 8213 AS id, 'row_8213' AS label;
DELETE FROM bench_t_22 WHERE id = 6;
WITH cte_8215 AS (SELECT 8215 AS n) SELECT n FROM cte_8215;
UPDATE bench_t_24 SET payload = 8216 WHERE id = 24;
INSERT INTO bench_t_25 (id, payload) VALUES (8217, 'O''Brien');
SELECT 8218 AS id, 'row_8218' AS label;
SELECT nested FROM t WHERE id IN (8219, 8220, 8221);
UPDATE bench_t_28 SET payload = 8220 WHERE id = 28;
SELECT * FROM "quoted_8221" WHERE col = E'esc\'8221';
DELETE FROM bench_t_30 WHERE id = 14;
# hash comment 8223
BEGIN; SELECT 8224; COMMIT;
SELECT `mysql_8225` FROM `tbl_25`;
$dz$ dollar body 8226 ; semicolon inside $dz$
SELECT * FROM "quoted_8227" WHERE col = E'esc\'8227';
# hash comment 8228
BEGIN; SELECT 8229; COMMIT;
-- line 8230: deterministic comment
SELECT [bracket_8231] FROM [dbo].[tbl_31];
-- line 8232: deterministic comment
UPDATE bench_t_41 SET payload = 8233 WHERE id = 9;
INSERT INTO bench_t_42 (id, payload) VALUES (8234, 'v8234');
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT [bracket_8236] FROM [dbo].[tbl_36];
UPDATE bench_t_45 SET payload = 8237 WHERE id = 13;
DELETE FROM bench_t_14 WHERE id = 14;
INSERT INTO bench_t_47 (id, payload) VALUES (8239, 'O''Brien');
# hash comment 8240
/* block header 8241 */
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT `mysql_8243` FROM `tbl_43`;
SELECT 8244 AS id, 'row_8244' AS label;
/* block header 8245 */
DELETE FROM bench_t_22 WHERE id = 6;
WITH cte_8247 AS (SELECT 8247 AS n) SELECT n FROM cte_8247;
$dz$ dollar body 8248 ; semicolon inside $dz$
SELECT * FROM "quoted_8249" WHERE col = E'esc\'8249';
/*
 * section 33
 * checksum c8ce
 */
-- line 8250: deterministic comment
# hash comment 8255
SELECT 8256 AS id, 'row_8256' AS label;
UPDATE bench_t_1 SET payload = 8257 WHERE id = 1;
-- line 8258: deterministic comment
$dz$ dollar body 8259 ; semicolon inside $dz$
# hash comment 8260
# hash comment 8261
# hash comment 8262
-- line 8263: deterministic comment
SELECT 8264 AS id, 'row_8264' AS label;
/* block header 8265 */
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
WITH cte_8268 AS (SELECT 8268 AS n) SELECT n FROM cte_8268;
INSERT INTO bench_t_77 (id, payload) VALUES (8269, 'v8269');
SELECT [bracket_8270] FROM [dbo].[tbl_30];
SELECT nested FROM t WHERE id IN (8271, 8272, 8273);
INSERT INTO bench_t_80 (id, payload) VALUES (8272, 'O''Brien');
SELECT nested FROM t WHERE id IN (8273, 8274, 8275);
SELECT * FROM "quoted_8274" WHERE col = E'esc\'8274';
UPDATE bench_t_19 SET payload = 8275 WHERE id = 19;
-- line 8276: deterministic comment
-- line 8277: deterministic comment
SELECT [bracket_8278] FROM [dbo].[tbl_38];
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT `mysql_8280` FROM `tbl_30`;
WITH cte_8281 AS (SELECT 8281 AS n) SELECT n FROM cte_8281;
BEGIN; SELECT 8282; COMMIT;
# hash comment 8283
DELETE FROM bench_t_28 WHERE id = 12;
SELECT * FROM "quoted_8285" WHERE col = E'esc\'8285';
DELETE FROM bench_t_30 WHERE id = 14;
SELECT nested FROM t WHERE id IN (8287, 8288, 8289);
INSERT INTO bench_t_96 (id, payload) VALUES (8288, 'v8288');
# hash comment 8289
UPDATE bench_t_34 SET payload = 8290 WHERE id = 2;
DELETE FROM bench_t_3 WHERE id = 3;
$dz$ dollar body 8292 ; semicolon inside $dz$
-- line 8293: deterministic comment
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
/* block header 8295 */
SELECT nested FROM t WHERE id IN (8296, 8297, 8298);
BEGIN; SELECT 8297; COMMIT;
/* block header 8298 */
BEGIN; SELECT 8299; COMMIT;
SELECT [bracket_8300] FROM [dbo].[tbl_20];
BEGIN; SELECT 8301; COMMIT;
INSERT INTO bench_t_110 (id, payload) VALUES (8302, 'v8302');
SELECT nested FROM t WHERE id IN (8303, 8304, 8305);
WITH cte_8304 AS (SELECT 8304 AS n) SELECT n FROM cte_8304;
WITH cte_8305 AS (SELECT 8305 AS n) SELECT n FROM cte_8305;
DELETE FROM bench_t_18 WHERE id = 2;
INSERT INTO bench_t_115 (id, payload) VALUES (8307, 'v8307');
BEGIN; SELECT 8308; COMMIT;
INSERT INTO bench_t_117 (id, payload) VALUES (8309, 'v8309');
SELECT 8310 AS id, 'row_8310' AS label;
$dz$ dollar body 8311 ; semicolon inside $dz$
BEGIN; SELECT 8312; COMMIT;
DELETE FROM bench_t_25 WHERE id = 9;
DELETE FROM bench_t_26 WHERE id = 10;
-- line 8315: deterministic comment
-- line 8316: deterministic comment
SELECT * FROM "quoted_8317" WHERE col = E'esc\'8317';
SELECT 8318 AS id, 'row_8318' AS label;
/* block header 8319 */
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
-- line 8321: deterministic comment
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
-- line 8323: deterministic comment
INSERT INTO bench_t_4 (id, payload) VALUES (8324, 'v8324');
SELECT [bracket_8325] FROM [dbo].[tbl_5];
# hash comment 8326
UPDATE bench_t_7 SET payload = 8327 WHERE id = 7;
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT [bracket_8329] FROM [dbo].[tbl_9];
SELECT nested FROM t WHERE id IN (8330, 8331, 8332);
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
-- line 8332: deterministic comment
BEGIN; SELECT 8333; COMMIT;
/* block header 8334 */
INSERT INTO bench_t_15 (id, payload) VALUES (8335, 'v8335');
DELETE FROM bench_t_16 WHERE id = 0;
UPDATE bench_t_17 SET payload = 8337 WHERE id = 17;
-- line 8338: deterministic comment
SELECT nested FROM t WHERE id IN (8339, 8340, 8341);
SELECT [bracket_8340] FROM [dbo].[tbl_20];
BEGIN; SELECT 8341; COMMIT;
INSERT INTO bench_t_22 (id, payload) VALUES (8342, 'v8342');
SELECT 8343 AS id, 'row_8343' AS label;
-- line 8344: deterministic comment
DELETE FROM bench_t_25 WHERE id = 9;
-- line 8346: deterministic comment
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT 8348 AS id, 'row_8348' AS label;
INSERT INTO bench_t_29 (id, payload) VALUES (8349, 'O''Brien');
DELETE FROM bench_t_30 WHERE id = 14;
# hash comment 8351
SELECT [bracket_8352] FROM [dbo].[tbl_32];
INSERT INTO bench_t_33 (id, payload) VALUES (8353, 'v8353');
$dz$ dollar body 8354 ; semicolon inside $dz$
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT nested FROM t WHERE id IN (8356, 8357, 8358);
/* block header 8357 */
BEGIN; SELECT 8358; COMMIT;
WITH cte_8359 AS (SELECT 8359 AS n) SELECT n FROM cte_8359;
$dz$ dollar body 8360 ; semicolon inside $dz$
SELECT 8361 AS id, 'row_8361' AS label;
# hash comment 8362
WITH cte_8363 AS (SELECT 8363 AS n) SELECT n FROM cte_8363;
# hash comment 8364
WITH cte_8365 AS (SELECT 8365 AS n) SELECT n FROM cte_8365;
WITH cte_8366 AS (SELECT 8366 AS n) SELECT n FROM cte_8366;
INSERT INTO bench_t_47 (id, payload) VALUES (8367, 'v8367');
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT [bracket_8369] FROM [dbo].[tbl_9];
UPDATE bench_t_50 SET payload = 8370 WHERE id = 18;
$dz$ dollar body 8371 ; semicolon inside $dz$
$dz$ dollar body 8372 ; semicolon inside $dz$
# hash comment 8373
UPDATE bench_t_54 SET payload = 8374 WHERE id = 22;
# hash comment 8375
$dz$ dollar body 8376 ; semicolon inside $dz$
SELECT * FROM "quoted_8377" WHERE col = E'esc\'8377';
# hash comment 8378
/* block header 8379 */
SELECT nested FROM t WHERE id IN (8380, 8381, 8382);
UPDATE bench_t_61 SET payload = 8381 WHERE id = 29;
DELETE FROM bench_t_30 WHERE id = 14;
BEGIN; SELECT 8383; COMMIT;
DELETE FROM bench_t_0 WHERE id = 0;
UPDATE bench_t_1 SET payload = 8385 WHERE id = 1;
UPDATE bench_t_2 SET payload = 8386 WHERE id = 2;
SELECT [bracket_8387] FROM [dbo].[tbl_27];
SELECT * FROM "quoted_8388" WHERE col = E'esc\'8388';
SELECT `mysql_8389` FROM `tbl_39`;
SELECT `mysql_8390` FROM `tbl_40`;
BEGIN; SELECT 8391; COMMIT;
# hash comment 8392
SELECT `mysql_8393` FROM `tbl_43`;
UPDATE bench_t_10 SET payload = 8394 WHERE id = 10;
SELECT nested FROM t WHERE id IN (8395, 8396, 8397);
SELECT `mysql_8396` FROM `tbl_46`;
BEGIN; SELECT 8397; COMMIT;
SELECT 8398 AS id, 'row_8398' AS label;
/* block header 8399 */
SELECT * FROM "quoted_8400" WHERE col = E'esc\'8400';
WITH cte_8401 AS (SELECT 8401 AS n) SELECT n FROM cte_8401;
SELECT `mysql_8402` FROM `tbl_2`;
WITH cte_8403 AS (SELECT 8403 AS n) SELECT n FROM cte_8403;
SELECT 8404 AS id, 'row_8404' AS label;
# hash comment 8405
-- line 8406: deterministic comment
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
BEGIN; SELECT 8408; COMMIT;
INSERT INTO bench_t_89 (id, payload) VALUES (8409, 'v8409');
UPDATE bench_t_26 SET payload = 8410 WHERE id = 26;
# hash comment 8411
SELECT [bracket_8412] FROM [dbo].[tbl_12];
INSERT INTO bench_t_93 (id, payload) VALUES (8413, 'v8413');
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
DELETE FROM bench_t_31 WHERE id = 15;
SELECT nested FROM t WHERE id IN (8416, 8417, 8418);
SELECT `mysql_8417` FROM `tbl_17`;
-- line 8418: deterministic comment
SELECT * FROM "quoted_8419" WHERE col = E'esc\'8419';
SELECT `mysql_8420` FROM `tbl_20`;
SELECT [bracket_8421] FROM [dbo].[tbl_21];
BEGIN; SELECT 8422; COMMIT;
SELECT nested FROM t WHERE id IN (8423, 8424, 8425);
/* block header 8424 */
WITH cte_8425 AS (SELECT 8425 AS n) SELECT n FROM cte_8425;
DELETE FROM bench_t_10 WHERE id = 10;
-- line 8427: deterministic comment
INSERT INTO bench_t_108 (id, payload) VALUES (8428, 'v8428');
$dz$ dollar body 8429 ; semicolon inside $dz$
UPDATE bench_t_46 SET payload = 8430 WHERE id = 14;
SELECT nested FROM t WHERE id IN (8431, 8432, 8433);
BEGIN; SELECT 8432; COMMIT;
SELECT `mysql_8433` FROM `tbl_33`;
$dz$ dollar body 8434 ; semicolon inside $dz$
DELETE FROM bench_t_19 WHERE id = 3;
/* block header 8436 */
WITH cte_8437 AS (SELECT 8437 AS n) SELECT n FROM cte_8437;
DELETE FROM bench_t_22 WHERE id = 6;
SELECT 8439 AS id, 'row_8439' AS label;
BEGIN; SELECT 8440; COMMIT;
INSERT INTO bench_t_121 (id, payload) VALUES (8441, 'v8441');
SELECT * FROM "quoted_8442" WHERE col = E'esc\'8442';
SELECT nested FROM t WHERE id IN (8443, 8444, 8445);
SELECT nested FROM t WHERE id IN (8444, 8445, 8446);
$dz$ dollar body 8445 ; semicolon inside $dz$
UPDATE bench_t_62 SET payload = 8446 WHERE id = 30;
$dz$ dollar body 8447 ; semicolon inside $dz$
BEGIN; SELECT 8448; COMMIT;
DELETE FROM bench_t_1 WHERE id = 1;
SELECT * FROM "quoted_8450" WHERE col = E'esc\'8450';
SELECT * FROM "quoted_8451" WHERE col = E'esc\'8451';
/* block header 8452 */
INSERT INTO bench_t_5 (id, payload) VALUES (8453, 'v8453');
DELETE FROM bench_t_6 WHERE id = 6;
SELECT `mysql_8455` FROM `tbl_5`;
SELECT nested FROM t WHERE id IN (8456, 8457, 8458);
SELECT `mysql_8457` FROM `tbl_7`;
SELECT `mysql_8458` FROM `tbl_8`;
/* block header 8459 */
# hash comment 8460
WITH cte_8461 AS (SELECT 8461 AS n) SELECT n FROM cte_8461;
SELECT 8462 AS id, 'row_8462' AS label;
INSERT INTO bench_t_15 (id, payload) VALUES (8463, 'v8463');
$dz$ dollar body 8464 ; semicolon inside $dz$
SELECT nested FROM t WHERE id IN (8465, 8466, 8467);
BEGIN; SELECT 8466; COMMIT;
BEGIN; SELECT 8467; COMMIT;
/* block header 8468 */
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
WITH cte_8470 AS (SELECT 8470 AS n) SELECT n FROM cte_8470;
UPDATE bench_t_23 SET payload = 8471 WHERE id = 23;
WITH cte_8472 AS (SELECT 8472 AS n) SELECT n FROM cte_8472;
SELECT 8473 AS id, 'row_8473' AS label;
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
BEGIN; SELECT 8475; COMMIT;
UPDATE bench_t_28 SET payload = 8476 WHERE id = 28;
UPDATE bench_t_29 SET payload = 8477 WHERE id = 29;
SELECT * FROM "quoted_8478" WHERE col = E'esc\'8478';
SELECT 8479 AS id, 'row_8479' AS label;
INSERT INTO bench_t_32 (id, payload) VALUES (8480, 'v8480');
SELECT nested FROM t WHERE id IN (8481, 8482, 8483);
-- line 8482: deterministic comment
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
BEGIN; SELECT 8485; COMMIT;
SELECT nested FROM t WHERE id IN (8486, 8487, 8488);
/* block header 8487 */
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
DELETE FROM bench_t_9 WHERE id = 9;
SELECT nested FROM t WHERE id IN (8490, 8491, 8492);
SELECT nested FROM t WHERE id IN (8491, 8492, 8493);
-- line 8492: deterministic comment
UPDATE bench_t_45 SET payload = 8493 WHERE id = 13;
INSERT INTO bench_t_46 (id, payload) VALUES (8494, 'v8494');
SELECT [bracket_8495] FROM [dbo].[tbl_15];
# hash comment 8496
DELETE FROM bench_t_17 WHERE id = 1;
BEGIN; SELECT 8498; COMMIT;
SELECT nested FROM t WHERE id IN (8499, 8500, 8501);
/*
 * section 34
 * checksum b19a
 */
WITH cte_8500 AS (SELECT 8500 AS n) SELECT n FROM cte_8500;
WITH cte_8505 AS (SELECT 8505 AS n) SELECT n FROM cte_8505;
SELECT 8506 AS id, 'row_8506' AS label;
BEGIN; SELECT 8507; COMMIT;
BEGIN; SELECT 8508; COMMIT;
SELECT * FROM "quoted_8509" WHERE col = E'esc\'8509';
/* block header 8510 */
SELECT 8511 AS id, 'row_8511' AS label;
DELETE FROM bench_t_0 WHERE id = 0;
SELECT `mysql_8513` FROM `tbl_13`;
# hash comment 8514
/* block header 8515 */
SELECT * FROM "quoted_8516" WHERE col = E'esc\'8516';
DELETE FROM bench_t_5 WHERE id = 5;
DELETE FROM bench_t_6 WHERE id = 6;
$dz$ dollar body 8519 ; semicolon inside $dz$
WITH cte_8520 AS (SELECT 8520 AS n) SELECT n FROM cte_8520;
INSERT INTO bench_t_73 (id, payload) VALUES (8521, 'v8521');
SELECT [bracket_8522] FROM [dbo].[tbl_2];
# hash comment 8523
-- line 8524: deterministic comment
/* block header 8525 */
SELECT `mysql_8526` FROM `tbl_26`;
-- line 8527: deterministic comment
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
/* block header 8529 */
/* block header 8530 */
-- line 8531: deterministic comment
WITH cte_8532 AS (SELECT 8532 AS n) SELECT n FROM cte_8532;
BEGIN; SELECT 8533; COMMIT;
SELECT `mysql_8534` FROM `tbl_34`;
$dz$ dollar body 8535 ; semicolon inside $dz$
INSERT INTO bench_t_88 (id, payload) VALUES (8536, 'O''Brien');
DELETE FROM bench_t_25 WHERE id = 9;
SELECT `mysql_8538` FROM `tbl_38`;
SELECT nested FROM t WHERE id IN (8539, 8540, 8541);
/* block header 8540 */
UPDATE bench_t_29 SET payload = 8541 WHERE id = 29;
UPDATE bench_t_30 SET payload = 8542 WHERE id = 30;
/* block header 8543 */
# hash comment 8544
WITH cte_8545 AS (SELECT 8545 AS n) SELECT n FROM cte_8545;
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT * FROM "quoted_8547" WHERE col = E'esc\'8547';
# hash comment 8548
/* block header 8549 */
SELECT * FROM "quoted_8550" WHERE col = E'esc\'8550';
DELETE FROM bench_t_7 WHERE id = 7;
UPDATE bench_t_40 SET payload = 8552 WHERE id = 8;
INSERT INTO bench_t_105 (id, payload) VALUES (8553, 'v8553');
SELECT `mysql_8554` FROM `tbl_4`;
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
# hash comment 8556
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT [bracket_8558] FROM [dbo].[tbl_38];
# hash comment 8559
SELECT `mysql_8560` FROM `tbl_10`;
/* block header 8561 */
-- line 8562: deterministic comment
SELECT * FROM "quoted_8563" WHERE col = E'esc\'8563';
/* block header 8564 */
SELECT * FROM "quoted_8565" WHERE col = E'esc\'8565';
-- line 8566: deterministic comment
# hash comment 8567
SELECT `mysql_8568` FROM `tbl_18`;
SELECT 8569 AS id, 'row_8569' AS label;
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
-- line 8571: deterministic comment
SELECT nested FROM t WHERE id IN (8572, 8573, 8574);
$dz$ dollar body 8573 ; semicolon inside $dz$
DELETE FROM bench_t_30 WHERE id = 14;
BEGIN; SELECT 8575; COMMIT;
SELECT nested FROM t WHERE id IN (8576, 8577, 8578);
SELECT nested FROM t WHERE id IN (8577, 8578, 8579);
SELECT [bracket_8578] FROM [dbo].[tbl_18];
SELECT [bracket_8579] FROM [dbo].[tbl_19];
INSERT INTO bench_t_4 (id, payload) VALUES (8580, 'O''Brien');
UPDATE bench_t_5 SET payload = 8581 WHERE id = 5;
SELECT 8582 AS id, 'row_8582' AS label;
SELECT nested FROM t WHERE id IN (8583, 8584, 8585);
-- line 8584: deterministic comment
SELECT 8585 AS id, 'row_8585' AS label;
/* block header 8586 */
WITH cte_8587 AS (SELECT 8587 AS n) SELECT n FROM cte_8587;
SELECT 8588 AS id, 'row_8588' AS label;
SELECT * FROM "quoted_8589" WHERE col = E'esc\'8589';
DELETE FROM bench_t_14 WHERE id = 14;
SELECT 8591 AS id, 'row_8591' AS label;
-- line 8592: deterministic comment
SELECT * FROM "quoted_8593" WHERE col = E'esc\'8593';
-- line 8594: deterministic comment
INSERT INTO bench_t_19 (id, payload) VALUES (8595, 'v8595');
UPDATE bench_t_20 SET payload = 8596 WHERE id = 20;
-- line 8597: deterministic comment
-- line 8598: deterministic comment
SELECT [bracket_8599] FROM [dbo].[tbl_39];
SELECT nested FROM t WHERE id IN (8600, 8601, 8602);
SELECT * FROM "quoted_8601" WHERE col = E'esc\'8601';
SELECT * FROM "quoted_8602" WHERE col = E'esc\'8602';
SELECT * FROM "quoted_8603" WHERE col = E'esc\'8603';
-- line 8604: deterministic comment
UPDATE bench_t_29 SET payload = 8605 WHERE id = 29;
INSERT INTO bench_t_30 (id, payload) VALUES (8606, 'v8606');
BEGIN; SELECT 8607; COMMIT;
SELECT 8608 AS id, 'row_8608' AS label;
-- line 8609: deterministic comment
SELECT `mysql_8610` FROM `tbl_10`;
-- line 8611: deterministic comment
SELECT 8612 AS id, 'row_8612' AS label;
SELECT [bracket_8613] FROM [dbo].[tbl_13];
SELECT * FROM "quoted_8614" WHERE col = E'esc\'8614';
INSERT INTO bench_t_39 (id, payload) VALUES (8615, 'v8615');
# hash comment 8616
SELECT `mysql_8617` FROM `tbl_17`;
BEGIN; SELECT 8618; COMMIT;
-- line 8619: deterministic comment
SELECT 8620 AS id, 'row_8620' AS label;
$dz$ dollar body 8621 ; semicolon inside $dz$
SELECT [bracket_8622] FROM [dbo].[tbl_22];
DELETE FROM bench_t_15 WHERE id = 15;
/* block header 8624 */
SELECT nested FROM t WHERE id IN (8625, 8626, 8627);
SELECT [bracket_8626] FROM [dbo].[tbl_26];
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
/* block header 8628 */
UPDATE bench_t_53 SET payload = 8629 WHERE id = 21;
# hash comment 8630
/* block header 8631 */
# hash comment 8632
SELECT * FROM "quoted_8633" WHERE col = E'esc\'8633';
INSERT INTO bench_t_58 (id, payload) VALUES (8634, 'v8634');
SELECT [bracket_8635] FROM [dbo].[tbl_35];
SELECT [bracket_8636] FROM [dbo].[tbl_36];
SELECT nested FROM t WHERE id IN (8637, 8638, 8639);
-- line 8638: deterministic comment
SELECT 8639 AS id, 'row_8639' AS label;
SELECT nested FROM t WHERE id IN (8640, 8641, 8642);
$dz$ dollar body 8641 ; semicolon inside $dz$
SELECT nested FROM t WHERE id IN (8642, 8643, 8644);
/* block header 8643 */
SELECT `mysql_8644` FROM `tbl_44`;
BEGIN; SELECT 8645; COMMIT;
WITH cte_8646 AS (SELECT 8646 AS n) SELECT n FROM cte_8646;
$dz$ dollar body 8647 ; semicolon inside $dz$
SELECT `mysql_8648` FROM `tbl_48`;
UPDATE bench_t_9 SET payload = 8649 WHERE id = 9;
SELECT [bracket_8650] FROM [dbo].[tbl_10];
# hash comment 8651
# hash comment 8652
DELETE FROM bench_t_13 WHERE id = 13;
DELETE FROM bench_t_14 WHERE id = 14;
$dz$ dollar body 8655 ; semicolon inside $dz$
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
DELETE FROM bench_t_17 WHERE id = 1;
BEGIN; SELECT 8658; COMMIT;
-- line 8659: deterministic comment
/* block header 8660 */
SELECT 8661 AS id, 'row_8661' AS label;
# hash comment 8662
UPDATE bench_t_23 SET payload = 8663 WHERE id = 23;
$dz$ dollar body 8664 ; semicolon inside $dz$
SELECT * FROM "quoted_8665" WHERE col = E'esc\'8665';
UPDATE bench_t_26 SET payload = 8666 WHERE id = 26;
SELECT `mysql_8667` FROM `tbl_17`;
INSERT INTO bench_t_92 (id, payload) VALUES (8668, 'O''Brien');
DELETE FROM bench_t_29 WHERE id = 13;
SELECT `mysql_8670` FROM `tbl_20`;
SELECT 8671 AS id, 'row_8671' AS label;
SELECT nested FROM t WHERE id IN (8672, 8673, 8674);
INSERT INTO bench_t_97 (id, payload) VALUES (8673, 'v8673');
$dz$ dollar body 8674 ; semicolon inside $dz$
SELECT `mysql_8675` FROM `tbl_25`;
WITH cte_8676 AS (SELECT 8676 AS n) SELECT n FROM cte_8676;
SELECT * FROM "quoted_8677" WHERE col = E'esc\'8677';
-- line 8678: deterministic comment
SELECT [bracket_8679] FROM [dbo].[tbl_39];
/* block header 8680 */
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT nested FROM t WHERE id IN (8682, 8683, 8684);
SELECT [bracket_8683] FROM [dbo].[tbl_3];
DELETE FROM bench_t_12 WHERE id = 12;
UPDATE bench_t_45 SET payload = 8685 WHERE id = 13;
SELECT [bracket_8686] FROM [dbo].[tbl_6];
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
WITH cte_8688 AS (SELECT 8688 AS n) SELECT n FROM cte_8688;
SELECT * FROM "quoted_8689" WHERE col = E'esc\'8689';
SELECT * FROM "quoted_8690" WHERE col = E'esc\'8690';
INSERT INTO bench_t_115 (id, payload) VALUES (8691, 'v8691');
BEGIN; SELECT 8692; COMMIT;
SELECT 8693 AS id, 'row_8693' AS label;
SELECT * FROM "quoted_8694" WHERE col = E'esc\'8694';
WITH cte_8695 AS (SELECT 8695 AS n) SELECT n FROM cte_8695;
/* block header 8696 */
-- line 8697: deterministic comment
SELECT nested FROM t WHERE id IN (8698, 8699, 8700);
SELECT `mysql_8699` FROM `tbl_49`;
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
DELETE FROM bench_t_29 WHERE id = 13;
SELECT nested FROM t WHERE id IN (8702, 8703, 8704);
DELETE FROM bench_t_31 WHERE id = 15;
-- line 8704: deterministic comment
-- line 8705: deterministic comment
SELECT * FROM "quoted_8706" WHERE col = E'esc\'8706';
/* block header 8707 */
SELECT * FROM "quoted_8708" WHERE col = E'esc\'8708';
-- line 8709: deterministic comment
SELECT [bracket_8710] FROM [dbo].[tbl_30];
$dz$ dollar body 8711 ; semicolon inside $dz$
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
DELETE FROM bench_t_9 WHERE id = 9;
/* block header 8714 */
BEGIN; SELECT 8715; COMMIT;
SELECT [bracket_8716] FROM [dbo].[tbl_36];
SELECT [bracket_8717] FROM [dbo].[tbl_37];
DELETE FROM bench_t_14 WHERE id = 14;
UPDATE bench_t_15 SET payload = 8719 WHERE id = 15;
INSERT INTO bench_t_16 (id, payload) VALUES (8720, 'v8720');
$dz$ dollar body 8721 ; semicolon inside $dz$
$dz$ dollar body 8722 ; semicolon inside $dz$
DELETE FROM bench_t_19 WHERE id = 3;
# hash comment 8724
UPDATE bench_t_21 SET payload = 8725 WHERE id = 21;
INSERT INTO bench_t_22 (id, payload) VALUES (8726, 'v8726');
SELECT * FROM "quoted_8727" WHERE col = E'esc\'8727';
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
INSERT INTO bench_t_25 (id, payload) VALUES (8729, 'v8729');
$dz$ dollar body 8730 ; semicolon inside $dz$
SELECT `mysql_8731` FROM `tbl_31`;
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
/* block header 8733 */
-- line 8734: deterministic comment
BEGIN; SELECT 8735; COMMIT;
INSERT INTO bench_t_32 (id, payload) VALUES (8736, 'v8736');
INSERT INTO bench_t_33 (id, payload) VALUES (8737, 'v8737');
WITH cte_8738 AS (SELECT 8738 AS n) SELECT n FROM cte_8738;
$dz$ dollar body 8739 ; semicolon inside $dz$
# hash comment 8740
SELECT 8741 AS id, 'row_8741' AS label;
SELECT [bracket_8742] FROM [dbo].[tbl_22];
WITH cte_8743 AS (SELECT 8743 AS n) SELECT n FROM cte_8743;
SELECT nested FROM t WHERE id IN (8744, 8745, 8746);
SELECT 8745 AS id, 'row_8745' AS label;
# hash comment 8746
SELECT * FROM "quoted_8747" WHERE col = E'esc\'8747';
INSERT INTO bench_t_44 (id, payload) VALUES (8748, 'v8748');
BEGIN; SELECT 8749; COMMIT;
/*
 * section 35
 * checksum ef42
 */
SELECT [bracket_8750] FROM [dbo].[tbl_30];
SELECT [bracket_8755] FROM [dbo].[tbl_35];
/* block header 8756 */
INSERT INTO bench_t_53 (id, payload) VALUES (8757, 'v8757');
SELECT `mysql_8758` FROM `tbl_8`;
DELETE FROM bench_t_23 WHERE id = 7;
SELECT [bracket_8760] FROM [dbo].[tbl_0];
SELECT `mysql_8761` FROM `tbl_11`;
WITH cte_8762 AS (SELECT 8762 AS n) SELECT n FROM cte_8762;
$dz$ dollar body 8763 ; semicolon inside $dz$
DELETE FROM bench_t_28 WHERE id = 12;
SELECT * FROM "quoted_8765" WHERE col = E'esc\'8765';
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
WITH cte_8767 AS (SELECT 8767 AS n) SELECT n FROM cte_8767;
SELECT [bracket_8768] FROM [dbo].[tbl_8];
SELECT 8769 AS id, 'row_8769' AS label;
# hash comment 8770
SELECT [bracket_8771] FROM [dbo].[tbl_11];
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
BEGIN; SELECT 8774; COMMIT;
DELETE FROM bench_t_7 WHERE id = 7;
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
INSERT INTO bench_t_73 (id, payload) VALUES (8777, 'v8777');
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
WITH cte_8779 AS (SELECT 8779 AS n) SELECT n FROM cte_8779;
SELECT 8780 AS id, 'row_8780' AS label;
WITH cte_8781 AS (SELECT 8781 AS n) SELECT n FROM cte_8781;
# hash comment 8782
BEGIN; SELECT 8783; COMMIT;
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT * FROM "quoted_8785" WHERE col = E'esc\'8785';
BEGIN; SELECT 8786; COMMIT;
/* block header 8787 */
INSERT INTO bench_t_84 (id, payload) VALUES (8788, 'v8788');
$dz$ dollar body 8789 ; semicolon inside $dz$
SELECT `mysql_8790` FROM `tbl_40`;
DELETE FROM bench_t_23 WHERE id = 7;
-- line 8792: deterministic comment
# hash comment 8793
# hash comment 8794
SELECT [bracket_8795] FROM [dbo].[tbl_35];
BEGIN; SELECT 8796; COMMIT;
UPDATE bench_t_29 SET payload = 8797 WHERE id = 29;
/* block header 8798 */
SELECT nested FROM t WHERE id IN (8799, 8800, 8801);
SELECT nested FROM t WHERE id IN (8800, 8801, 8802);
BEGIN; SELECT 8801; COMMIT;
DELETE FROM bench_t_2 WHERE id = 2;
INSERT INTO bench_t_99 (id, payload) VALUES (8803, 'v8803');
DELETE FROM bench_t_4 WHERE id = 4;
SELECT `mysql_8805` FROM `tbl_5`;
SELECT `mysql_8806` FROM `tbl_6`;
BEGIN; SELECT 8807; COMMIT;
DELETE FROM bench_t_8 WHERE id = 8;
# hash comment 8809
INSERT INTO bench_t_106 (id, payload) VALUES (8810, 'v8810');
SELECT 8811 AS id, 'row_8811' AS label;
SELECT nested FROM t WHERE id IN (8812, 8813, 8814);
WITH cte_8813 AS (SELECT 8813 AS n) SELECT n FROM cte_8813;
UPDATE bench_t_46 SET payload = 8814 WHERE id = 14;
DELETE FROM bench_t_15 WHERE id = 15;
SELECT nested FROM t WHERE id IN (8816, 8817, 8818);
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
$dz$ dollar body 8818 ; semicolon inside $dz$
SELECT `mysql_8819` FROM `tbl_19`;
$dz$ dollar body 8820 ; semicolon inside $dz$
BEGIN; SELECT 8821; COMMIT;
SELECT * FROM "quoted_8822" WHERE col = E'esc\'8822';
UPDATE bench_t_55 SET payload = 8823 WHERE id = 23;
WITH cte_8824 AS (SELECT 8824 AS n) SELECT n FROM cte_8824;
WITH cte_8825 AS (SELECT 8825 AS n) SELECT n FROM cte_8825;
SELECT [bracket_8826] FROM [dbo].[tbl_26];
BEGIN; SELECT 8827; COMMIT;
SELECT 8828 AS id, 'row_8828' AS label;
WITH cte_8829 AS (SELECT 8829 AS n) SELECT n FROM cte_8829;
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT [bracket_8831] FROM [dbo].[tbl_31];
SELECT 8832 AS id, 'row_8832' AS label;
# hash comment 8833
SELECT nested FROM t WHERE id IN (8834, 8835, 8836);
WITH cte_8835 AS (SELECT 8835 AS n) SELECT n FROM cte_8835;
$dz$ dollar body 8836 ; semicolon inside $dz$
# hash comment 8837
DELETE FROM bench_t_6 WHERE id = 6;
# hash comment 8839
UPDATE bench_t_8 SET payload = 8840 WHERE id = 8;
SELECT [bracket_8841] FROM [dbo].[tbl_1];
SELECT `mysql_8842` FROM `tbl_42`;
SELECT * FROM "quoted_8843" WHERE col = E'esc\'8843';
SELECT * FROM "quoted_8844" WHERE col = E'esc\'8844';
SELECT 8845 AS id, 'row_8845' AS label;
SELECT [bracket_8846] FROM [dbo].[tbl_6];
# hash comment 8847
SELECT nested FROM t WHERE id IN (8848, 8849, 8850);
BEGIN; SELECT 8849; COMMIT;
# hash comment 8850
SELECT * FROM "quoted_8851" WHERE col = E'esc\'8851';
INSERT INTO bench_t_20 (id, payload) VALUES (8852, 'v8852');
SELECT 8853 AS id, 'row_8853' AS label;
-- line 8854: deterministic comment
# hash comment 8855
SELECT nested FROM t WHERE id IN (8856, 8857, 8858);
SELECT [bracket_8857] FROM [dbo].[tbl_17];
$dz$ dollar body 8858 ; semicolon inside $dz$
# hash comment 8859
BEGIN; SELECT 8860; COMMIT;
UPDATE bench_t_29 SET payload = 8861 WHERE id = 29;
UPDATE bench_t_30 SET payload = 8862 WHERE id = 30;
BEGIN; SELECT 8863; COMMIT;
WITH cte_8864 AS (SELECT 8864 AS n) SELECT n FROM cte_8864;
WITH cte_8865 AS (SELECT 8865 AS n) SELECT n FROM cte_8865;
UPDATE bench_t_34 SET payload = 8866 WHERE id = 2;
SELECT * FROM "quoted_8867" WHERE col = E'esc\'8867';
SELECT 8868 AS id, 'row_8868' AS label;
/* block header 8869 */
SELECT [bracket_8870] FROM [dbo].[tbl_30];
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
/* block header 8872 */
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
INSERT INTO bench_t_42 (id, payload) VALUES (8874, 'v8874');
BEGIN; SELECT 8875; COMMIT;
SELECT nested FROM t WHERE id IN (8876, 8877, 8878);
SELECT `mysql_8877` FROM `tbl_27`;
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
# hash comment 8879
# hash comment 8880
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT `mysql_8882` FROM `tbl_32`;
$dz$ dollar body 8883 ; semicolon inside $dz$
$dz$ dollar body 8884 ; semicolon inside $dz$
WITH cte_8885 AS (SELECT 8885 AS n) SELECT n FROM cte_8885;
/* block header 8886 */
SELECT 8887 AS id, 'row_8887' AS label;
# hash comment 8888
SELECT 8889 AS id, 'row_8889' AS label;
-- line 8890: deterministic comment
SELECT nested FROM t WHERE id IN (8891, 8892, 8893);
INSERT INTO bench_t_60 (id, payload) VALUES (8892, 'v8892');
BEGIN; SELECT 8893; COMMIT;
BEGIN; SELECT 8894; COMMIT;
-- line 8895: deterministic comment
SELECT nested FROM t WHERE id IN (8896, 8897, 8898);
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
INSERT INTO bench_t_66 (id, payload) VALUES (8898, 'v8898');
SELECT 8899 AS id, 'row_8899' AS label;
SELECT nested FROM t WHERE id IN (8900, 8901, 8902);
# hash comment 8901
/* block header 8902 */
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
DELETE FROM bench_t_8 WHERE id = 8;
$dz$ dollar body 8905 ; semicolon inside $dz$
INSERT INTO bench_t_74 (id, payload) VALUES (8906, 'v8906');
DELETE FROM bench_t_11 WHERE id = 11;
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
WITH cte_8909 AS (SELECT 8909 AS n) SELECT n FROM cte_8909;
SELECT [bracket_8910] FROM [dbo].[tbl_30];
SELECT * FROM "quoted_8911" WHERE col = E'esc\'8911';
UPDATE bench_t_16 SET payload = 8912 WHERE id = 16;
SELECT `mysql_8913` FROM `tbl_13`;
DELETE FROM bench_t_18 WHERE id = 2;
/* block header 8915 */
INSERT INTO bench_t_84 (id, payload) VALUES (8916, 'v8916');
/* block header 8917 */
$dz$ dollar body 8918 ; semicolon inside $dz$
WITH cte_8919 AS (SELECT 8919 AS n) SELECT n FROM cte_8919;
SELECT nested FROM t WHERE id IN (8920, 8921, 8922);
$dz$ dollar body 8921 ; semicolon inside $dz$
-- line 8922: deterministic comment
SELECT * FROM "quoted_8923" WHERE col = E'esc\'8923';
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT nested FROM t WHERE id IN (8925, 8926, 8927);
INSERT INTO bench_t_94 (id, payload) VALUES (8926, 'v8926');
WITH cte_8927 AS (SELECT 8927 AS n) SELECT n FROM cte_8927;
UPDATE bench_t_32 SET payload = 8928 WHERE id = 0;
BEGIN; SELECT 8929; COMMIT;
-- line 8930: deterministic comment
SELECT `mysql_8931` FROM `tbl_31`;
/* block header 8932 */
SELECT nested FROM t WHERE id IN (8933, 8934, 8935);
INSERT INTO bench_t_102 (id, payload) VALUES (8934, 'v8934');
UPDATE bench_t_39 SET payload = 8935 WHERE id = 7;
WITH cte_8936 AS (SELECT 8936 AS n) SELECT n FROM cte_8936;
SELECT `mysql_8937` FROM `tbl_37`;
WITH cte_8938 AS (SELECT 8938 AS n) SELECT n FROM cte_8938;
-- line 8939: deterministic comment
BEGIN; SELECT 8940; COMMIT;
WITH cte_8941 AS (SELECT 8941 AS n) SELECT n FROM cte_8941;
SELECT nested FROM t WHERE id IN (8942, 8943, 8944);
SELECT nested FROM t WHERE id IN (8943, 8944, 8945);
SELECT `mysql_8944` FROM `tbl_44`;
/* block header 8945 */
SELECT nested FROM t WHERE id IN (8946, 8947, 8948);
SELECT 8947 AS id, 'row_8947' AS label;
UPDATE bench_t_52 SET payload = 8948 WHERE id = 20;
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
/* block header 8950 */
DELETE FROM bench_t_23 WHERE id = 7;
SELECT `mysql_8952` FROM `tbl_2`;
SELECT nested FROM t WHERE id IN (8953, 8954, 8955);
SELECT 8954 AS id, 'row_8954' AS label;
SELECT `mysql_8955` FROM `tbl_5`;
BEGIN; SELECT 8956; COMMIT;
SELECT `mysql_8957` FROM `tbl_7`;
/* block header 8958 */
# hash comment 8959
/* block header 8960 */
SELECT [bracket_8961] FROM [dbo].[tbl_1];
WITH cte_8962 AS (SELECT 8962 AS n) SELECT n FROM cte_8962;
SELECT [bracket_8963] FROM [dbo].[tbl_3];
# hash comment 8964
/* block header 8965 */
WITH cte_8966 AS (SELECT 8966 AS n) SELECT n FROM cte_8966;
INSERT INTO bench_t_7 (id, payload) VALUES (8967, 'v8967');
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
WITH cte_8969 AS (SELECT 8969 AS n) SELECT n FROM cte_8969;
# hash comment 8970
-- line 8971: deterministic comment
$dz$ dollar body 8972 ; semicolon inside $dz$
SELECT nested FROM t WHERE id IN (8973, 8974, 8975);
DELETE FROM bench_t_14 WHERE id = 14;
SELECT 8975 AS id, 'row_8975' AS label;
UPDATE bench_t_16 SET payload = 8976 WHERE id = 16;
-- line 8977: deterministic comment
/* block header 8978 */
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
# hash comment 8980
DELETE FROM bench_t_21 WHERE id = 5;
DELETE FROM bench_t_22 WHERE id = 6;
SELECT [bracket_8983] FROM [dbo].[tbl_23];
SELECT 8984 AS id, 'row_8984' AS label;
DELETE FROM bench_t_25 WHERE id = 9;
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT [bracket_8987] FROM [dbo].[tbl_27];
UPDATE bench_t_28 SET payload = 8988 WHERE id = 28;
$dz$ dollar body 8989 ; semicolon inside $dz$
INSERT INTO bench_t_30 (id, payload) VALUES (8990, 'v8990');
SELECT 8991 AS id, 'row_8991' AS label;
DELETE FROM bench_t_0 WHERE id = 0;
$dz$ dollar body 8993 ; semicolon inside $dz$
BEGIN; SELECT 8994; COMMIT;
SELECT * FROM "quoted_8995" WHERE col = E'esc\'8995';
$dz$ dollar body 8996 ; semicolon inside $dz$
UPDATE bench_t_37 SET payload = 8997 WHERE id = 5;
SELECT * FROM "quoted_8998" WHERE col = E'esc\'8998';
# hash comment 8999
/*
 * section 36
 * checksum 59f9
 */
SELECT * FROM "quoted_9000" WHERE col = E'esc\'9000';
SELECT 9005 AS id, 'row_9005' AS label;
-- line 9006: deterministic comment
INSERT INTO bench_t_47 (id, payload) VALUES (9007, 'v9007');
# hash comment 9008
SELECT nested FROM t WHERE id IN (9009, 9010, 9011);
-- line 9010: deterministic comment
SELECT 9011 AS id, 'row_9011' AS label;
-- line 9012: deterministic comment
/* block header 9013 */
SELECT [bracket_9014] FROM [dbo].[tbl_14];
# hash comment 9015
# hash comment 9016
BEGIN; SELECT 9017; COMMIT;
SELECT nested FROM t WHERE id IN (9018, 9019, 9020);
DELETE FROM bench_t_27 WHERE id = 11;
WITH cte_9020 AS (SELECT 9020 AS n) SELECT n FROM cte_9020;
SELECT nested FROM t WHERE id IN (9021, 9022, 9023);
-- line 9022: deterministic comment
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT 9024 AS id, 'row_9024' AS label;
DELETE FROM bench_t_1 WHERE id = 1;
SELECT 9026 AS id, 'row_9026' AS label;
UPDATE bench_t_3 SET payload = 9027 WHERE id = 3;
SELECT * FROM "quoted_9028" WHERE col = E'esc\'9028';
DELETE FROM bench_t_5 WHERE id = 5;
INSERT INTO bench_t_70 (id, payload) VALUES (9030, 'v9030');
SELECT 9031 AS id, 'row_9031' AS label;
$dz$ dollar body 9032 ; semicolon inside $dz$
DELETE FROM bench_t_9 WHERE id = 9;
$dz$ dollar body 9034 ; semicolon inside $dz$
SELECT [bracket_9035] FROM [dbo].[tbl_35];
SELECT * FROM "quoted_9036" WHERE col = E'esc\'9036';
INSERT INTO bench_t_77 (id, payload) VALUES (9037, 'v9037');
DELETE FROM bench_t_14 WHERE id = 14;
WITH cte_9039 AS (SELECT 9039 AS n) SELECT n FROM cte_9039;
DELETE FROM bench_t_16 WHERE id = 0;
SELECT nested FROM t WHERE id IN (9041, 9042, 9043);
SELECT * FROM "quoted_9042" WHERE col = E'esc\'9042';
# hash comment 9043
SELECT [bracket_9044] FROM [dbo].[tbl_4];
UPDATE bench_t_21 SET payload = 9045 WHERE id = 21;
BEGIN; SELECT 9046; COMMIT;
-- line 9047: deterministic comment
SELECT `mysql_9048` FROM `tbl_48`;
SELECT `mysql_9049` FROM `tbl_49`;
SELECT * FROM "quoted_9050" WHERE col = E'esc\'9050';
SELECT nested FROM t WHERE id IN (9051, 9052, 9053);
DELETE FROM bench_t_28 WHERE id = 12;
SELECT [bracket_9053] FROM [dbo].[tbl_13];
WITH cte_9054 AS (SELECT 9054 AS n) SELECT n FROM cte_9054;
$dz$ dollar body 9055 ; semicolon inside $dz$
INSERT INTO bench_t_96 (id, payload) VALUES (9056, 'v9056');
-- line 9057: deterministic comment
UPDATE bench_t_34 SET payload = 9058 WHERE id = 2;
BEGIN; SELECT 9059; COMMIT;
SELECT nested FROM t WHERE id IN (9060, 9061, 9062);
BEGIN; SELECT 9061; COMMIT;
DELETE FROM bench_t_6 WHERE id = 6;
WITH cte_9063 AS (SELECT 9063 AS n) SELECT n FROM cte_9063;
INSERT INTO bench_t_104 (id, payload) VALUES (9064, 'O''Brien');
INSERT INTO bench_t_105 (id, payload) VALUES (9065, 'v9065');
WITH cte_9066 AS (SELECT 9066 AS n) SELECT n FROM cte_9066;
SELECT 9067 AS id, 'row_9067' AS label;
WITH cte_9068 AS (SELECT 9068 AS n) SELECT n FROM cte_9068;
SELECT 9069 AS id, 'row_9069' AS label;
BEGIN; SELECT 9070; COMMIT;
/* block header 9071 */
WITH cte_9072 AS (SELECT 9072 AS n) SELECT n FROM cte_9072;
UPDATE bench_t_49 SET payload = 9073 WHERE id = 17;
-- line 9074: deterministic comment
BEGIN; SELECT 9075; COMMIT;
SELECT [bracket_9076] FROM [dbo].[tbl_36];
SELECT nested FROM t WHERE id IN (9077, 9078, 9079);
WITH cte_9078 AS (SELECT 9078 AS n) SELECT n FROM cte_9078;
SELECT nested FROM t WHERE id IN (9079, 9080, 9081);
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT `mysql_9081` FROM `tbl_31`;
$dz$ dollar body 9082 ; semicolon inside $dz$
SELECT `mysql_9083` FROM `tbl_33`;
/* block header 9084 */
-- line 9085: deterministic comment
SELECT 9086 AS id, 'row_9086' AS label;
SELECT * FROM "quoted_9087" WHERE col = E'esc\'9087';
SELECT [bracket_9088] FROM [dbo].[tbl_8];
-- line 9089: deterministic comment
/* block header 9090 */
-- line 9091: deterministic comment
SELECT `mysql_9092` FROM `tbl_42`;
$dz$ dollar body 9093 ; semicolon inside $dz$
DELETE FROM bench_t_6 WHERE id = 6;
SELECT [bracket_9095] FROM [dbo].[tbl_15];
-- line 9096: deterministic comment
# hash comment 9097
SELECT `mysql_9098` FROM `tbl_48`;
-- line 9099: deterministic comment
$dz$ dollar body 9100 ; semicolon inside $dz$
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT * FROM "quoted_9102" WHERE col = E'esc\'9102';
/* block header 9103 */
/* block header 9104 */
UPDATE bench_t_17 SET payload = 9105 WHERE id = 17;
DELETE FROM bench_t_18 WHERE id = 2;
WITH cte_9107 AS (SELECT 9107 AS n) SELECT n FROM cte_9107;
SELECT `mysql_9108` FROM `tbl_8`;
SELECT nested FROM t WHERE id IN (9109, 9110, 9111);
SELECT [bracket_9110] FROM [dbo].[tbl_30];
SELECT nested FROM t WHERE id IN (9111, 9112, 9113);
SELECT `mysql_9112` FROM `tbl_12`;
SELECT `mysql_9113` FROM `tbl_13`;
DELETE FROM bench_t_26 WHERE id = 10;
-- line 9115: deterministic comment
$dz$ dollar body 9116 ; semicolon inside $dz$
$dz$ dollar body 9117 ; semicolon inside $dz$
SELECT * FROM "quoted_9118" WHERE col = E'esc\'9118';
SELECT 9119 AS id, 'row_9119' AS label;
# hash comment 9120
SELECT [bracket_9121] FROM [dbo].[tbl_1];
# hash comment 9122
SELECT `mysql_9123` FROM `tbl_23`;
/* block header 9124 */
SELECT nested FROM t WHERE id IN (9125, 9126, 9127);
# hash comment 9126
# hash comment 9127
WITH cte_9128 AS (SELECT 9128 AS n) SELECT n FROM cte_9128;
BEGIN; SELECT 9129; COMMIT;
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT [bracket_9131] FROM [dbo].[tbl_11];
WITH cte_9132 AS (SELECT 9132 AS n) SELECT n FROM cte_9132;
SELECT [bracket_9133] FROM [dbo].[tbl_13];
-- line 9134: deterministic comment
SELECT * FROM "quoted_9135" WHERE col = E'esc\'9135';
SELECT `mysql_9136` FROM `tbl_36`;
WITH cte_9137 AS (SELECT 9137 AS n) SELECT n FROM cte_9137;
# hash comment 9138
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
UPDATE bench_t_52 SET payload = 9140 WHERE id = 20;
SELECT nested FROM t WHERE id IN (9141, 9142, 9143);
SELECT [bracket_9142] FROM [dbo].[tbl_22];
UPDATE bench_t_55 SET payload = 9143 WHERE id = 23;
$dz$ dollar body 9144 ; semicolon inside $dz$
SELECT nested FROM t WHERE id IN (9145, 9146, 9147);
SELECT nested FROM t WHERE id IN (9146, 9147, 9148);
INSERT INTO bench_t_59 (id, payload) VALUES (9147, 'v9147');
SELECT * FROM "quoted_9148" WHERE col = E'esc\'9148';
-- line 9149: deterministic comment
SELECT `mysql_9150` FROM `tbl_0`;
# hash comment 9151
SELECT 9152 AS id, 'row_9152' AS label;
SELECT `mysql_9153` FROM `tbl_3`;
SELECT `mysql_9154` FROM `tbl_4`;
INSERT INTO bench_t_67 (id, payload) VALUES (9155, 'v9155');
SELECT * FROM "quoted_9156" WHERE col = E'esc\'9156';
SELECT 9157 AS id, 'row_9157' AS label;
SELECT [bracket_9158] FROM [dbo].[tbl_38];
DELETE FROM bench_t_7 WHERE id = 7;
WITH cte_9160 AS (SELECT 9160 AS n) SELECT n FROM cte_9160;
/* block header 9161 */
WITH cte_9162 AS (SELECT 9162 AS n) SELECT n FROM cte_9162;
SELECT nested FROM t WHERE id IN (9163, 9164, 9165);
DELETE FROM bench_t_12 WHERE id = 12;
BEGIN; SELECT 9165; COMMIT;
WITH cte_9166 AS (SELECT 9166 AS n) SELECT n FROM cte_9166;
/* block header 9167 */
SELECT `mysql_9168` FROM `tbl_18`;
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
BEGIN; SELECT 9170; COMMIT;
UPDATE bench_t_19 SET payload = 9171 WHERE id = 19;
SELECT * FROM "quoted_9172" WHERE col = E'esc\'9172';
DELETE FROM bench_t_21 WHERE id = 5;
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
-- line 9175: deterministic comment
WITH cte_9176 AS (SELECT 9176 AS n) SELECT n FROM cte_9176;
/* block header 9177 */
SELECT nested FROM t WHERE id IN (9178, 9179, 9180);
INSERT INTO bench_t_91 (id, payload) VALUES (9179, 'v9179');
INSERT INTO bench_t_92 (id, payload) VALUES (9180, 'v9180');
/* block header 9181 */
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
UPDATE bench_t_31 SET payload = 9183 WHERE id = 31;
$dz$ dollar body 9184 ; semicolon inside $dz$
BEGIN; SELECT 9185; COMMIT;
INSERT INTO bench_t_98 (id, payload) VALUES (9186, 'v9186');
SELECT * FROM "quoted_9187" WHERE col = E'esc\'9187';
/* block header 9188 */
/* block header 9189 */
SELECT `mysql_9190` FROM `tbl_40`;
SELECT nested FROM t WHERE id IN (9191, 9192, 9193);
SELECT 9192 AS id, 'row_9192' AS label;
SELECT 9193 AS id, 'row_9193' AS label;
SELECT nested FROM t WHERE id IN (9194, 9195, 9196);
/* block header 9195 */
BEGIN; SELECT 9196; COMMIT;
$dz$ dollar body 9197 ; semicolon inside $dz$
# hash comment 9198
SELECT nested FROM t WHERE id IN (9199, 9200, 9201);
SELECT * FROM "quoted_9200" WHERE col = E'esc\'9200';
/* block header 9201 */
# hash comment 9202
INSERT INTO bench_t_115 (id, payload) VALUES (9203, 'v9203');
SELECT nested FROM t WHERE id IN (9204, 9205, 9206);
UPDATE bench_t_53 SET payload = 9205 WHERE id = 21;
SELECT * FROM "quoted_9206" WHERE col = E'esc\'9206';
-- line 9207: deterministic comment
BEGIN; SELECT 9208; COMMIT;
SELECT [bracket_9209] FROM [dbo].[tbl_9];
UPDATE bench_t_58 SET payload = 9210 WHERE id = 26;
-- line 9211: deterministic comment
WITH cte_9212 AS (SELECT 9212 AS n) SELECT n FROM cte_9212;
-- line 9213: deterministic comment
UPDATE bench_t_62 SET payload = 9214 WHERE id = 30;
UPDATE bench_t_63 SET payload = 9215 WHERE id = 31;
SELECT 9216 AS id, 'row_9216' AS label;
WITH cte_9217 AS (SELECT 9217 AS n) SELECT n FROM cte_9217;
-- line 9218: deterministic comment
SELECT 9219 AS id, 'row_9219' AS label;
BEGIN; SELECT 9220; COMMIT;
WITH cte_9221 AS (SELECT 9221 AS n) SELECT n FROM cte_9221;
WITH cte_9222 AS (SELECT 9222 AS n) SELECT n FROM cte_9222;
BEGIN; SELECT 9223; COMMIT;
BEGIN; SELECT 9224; COMMIT;
SELECT * FROM "quoted_9225" WHERE col = E'esc\'9225';
DELETE FROM bench_t_10 WHERE id = 10;
# hash comment 9227
BEGIN; SELECT 9228; COMMIT;
SELECT [bracket_9229] FROM [dbo].[tbl_29];
$dz$ dollar body 9230 ; semicolon inside $dz$
DELETE FROM bench_t_15 WHERE id = 15;
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
-- line 9233: deterministic comment
$dz$ dollar body 9234 ; semicolon inside $dz$
-- line 9235: deterministic comment
INSERT INTO bench_t_20 (id, payload) VALUES (9236, 'v9236');
SELECT * FROM "quoted_9237" WHERE col = E'esc\'9237';
SELECT nested FROM t WHERE id IN (9238, 9239, 9240);
-- line 9239: deterministic comment
SELECT `mysql_9240` FROM `tbl_40`;
SELECT `mysql_9241` FROM `tbl_41`;
SELECT [bracket_9242] FROM [dbo].[tbl_2];
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT [bracket_9244] FROM [dbo].[tbl_4];
SELECT nested FROM t WHERE id IN (9245, 9246, 9247);
# hash comment 9246
SELECT 9247 AS id, 'row_9247' AS label;
INSERT INTO bench_t_32 (id, payload) VALUES (9248, 'v9248');
WITH cte_9249 AS (SELECT 9249 AS n) SELECT n FROM cte_9249;
/*
 * section 37
 * checksum 2687
 */
$dz$ dollar body 9250 ; semicolon inside $dz$
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT 9256 AS id, 'row_9256' AS label;
DELETE FROM bench_t_9 WHERE id = 9;
# hash comment 9258
SELECT nested FROM t WHERE id IN (9259, 9260, 9261);
SELECT [bracket_9260] FROM [dbo].[tbl_20];
DELETE FROM bench_t_13 WHERE id = 13;
UPDATE bench_t_46 SET payload = 9262 WHERE id = 14;
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
# hash comment 9264
BEGIN; SELECT 9265; COMMIT;
WITH cte_9266 AS (SELECT 9266 AS n) SELECT n FROM cte_9266;
INSERT INTO bench_t_51 (id, payload) VALUES (9267, 'v9267');
BEGIN; SELECT 9268; COMMIT;
SELECT * FROM "quoted_9269" WHERE col = E'esc\'9269';
$dz$ dollar body 9270 ; semicolon inside $dz$
DELETE FROM bench_t_23 WHERE id = 7;
INSERT INTO bench_t_56 (id, payload) VALUES (9272, 'v9272');
# hash comment 9273
SELECT `mysql_9274` FROM `tbl_24`;
INSERT INTO bench_t_59 (id, payload) VALUES (9275, 'v9275');
SELECT 9276 AS id, 'row_9276' AS label;
SELECT [bracket_9277] FROM [dbo].[tbl_37];
WITH cte_9278 AS (SELECT 9278 AS n) SELECT n FROM cte_9278;
SELECT `mysql_9279` FROM `tbl_29`;
/* block header 9280 */
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
INSERT INTO bench_t_66 (id, payload) VALUES (9282, 'v9282');
/* block header 9283 */
$dz$ dollar body 9284 ; semicolon inside $dz$
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
-- line 9286: deterministic comment
/* block header 9287 */
SELECT `mysql_9288` FROM `tbl_38`;
SELECT nested FROM t WHERE id IN (9289, 9290, 9291);
# hash comment 9290
SELECT `mysql_9291` FROM `tbl_41`;
SELECT 9292 AS id, 'row_9292' AS label;
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT 9294 AS id, 'row_9294' AS label;
BEGIN; SELECT 9295; COMMIT;
WITH cte_9296 AS (SELECT 9296 AS n) SELECT n FROM cte_9296;
INSERT INTO bench_t_81 (id, payload) VALUES (9297, 'v9297');
SELECT * FROM "quoted_9298" WHERE col = E'esc\'9298';
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
BEGIN; SELECT 9300; COMMIT;
SELECT 9301 AS id, 'row_9301' AS label;
$dz$ dollar body 9302 ; semicolon inside $dz$
BEGIN; SELECT 9303; COMMIT;
BEGIN; SELECT 9304; COMMIT;
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
# hash comment 9306
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
DELETE FROM bench_t_29 WHERE id = 13;
-- line 9310: deterministic comment
INSERT INTO bench_t_95 (id, payload) VALUES (9311, 'v9311');
SELECT [bracket_9312] FROM [dbo].[tbl_32];
SELECT 9313 AS id, 'row_9313' AS label;
$dz$ dollar body 9314 ; semicolon inside $dz$
DELETE FROM bench_t_3 WHERE id = 3;
SELECT * FROM "quoted_9316" WHERE col = E'esc\'9316';
SELECT 9317 AS id, 'row_9317' AS label;
-- line 9318: deterministic comment
/* block header 9319 */
$dz$ dollar body 9320 ; semicolon inside $dz$
$dz$ dollar body 9321 ; semicolon inside $dz$
BEGIN; SELECT 9322; COMMIT;
BEGIN; SELECT 9323; COMMIT;
SELECT * FROM "quoted_9324" WHERE col = E'esc\'9324';
WITH cte_9325 AS (SELECT 9325 AS n) SELECT n FROM cte_9325;
WITH cte_9326 AS (SELECT 9326 AS n) SELECT n FROM cte_9326;
BEGIN; SELECT 9327; COMMIT;
$dz$ dollar body 9328 ; semicolon inside $dz$
SELECT `mysql_9329` FROM `tbl_29`;
UPDATE bench_t_50 SET payload = 9330 WHERE id = 18;
WITH cte_9331 AS (SELECT 9331 AS n) SELECT n FROM cte_9331;
SELECT `mysql_9332` FROM `tbl_32`;
DELETE FROM bench_t_21 WHERE id = 5;
INSERT INTO bench_t_118 (id, payload) VALUES (9334, 'v9334');
-- line 9335: deterministic comment
SELECT * FROM "quoted_9336" WHERE col = E'esc\'9336';
SELECT `mysql_9337` FROM `tbl_37`;
INSERT INTO bench_t_122 (id, payload) VALUES (9338, 'v9338');
SELECT * FROM "quoted_9339" WHERE col = E'esc\'9339';
SELECT `mysql_9340` FROM `tbl_40`;
WITH cte_9341 AS (SELECT 9341 AS n) SELECT n FROM cte_9341;
/* block header 9342 */
SELECT [bracket_9343] FROM [dbo].[tbl_23];
BEGIN; SELECT 9344; COMMIT;
SELECT `mysql_9345` FROM `tbl_45`;
SELECT * FROM "quoted_9346" WHERE col = E'esc\'9346';
UPDATE bench_t_3 SET payload = 9347 WHERE id = 3;
SELECT [bracket_9348] FROM [dbo].[tbl_28];
-- line 9349: deterministic comment
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
DELETE FROM bench_t_7 WHERE id = 7;
SELECT nested FROM t WHERE id IN (9352, 9353, 9354);
DELETE FROM bench_t_9 WHERE id = 9;
BEGIN; SELECT 9354; COMMIT;
DELETE FROM bench_t_11 WHERE id = 11;
BEGIN; SELECT 9356; COMMIT;
$dz$ dollar body 9357 ; semicolon inside $dz$
# hash comment 9358
UPDATE bench_t_15 SET payload = 9359 WHERE id = 15;
SELECT `mysql_9360` FROM `tbl_10`;
SELECT [bracket_9361] FROM [dbo].[tbl_1];
SELECT `mysql_9362` FROM `tbl_12`;
SELECT * FROM "quoted_9363" WHERE col = E'esc\'9363';
SELECT [bracket_9364] FROM [dbo].[tbl_4];
SELECT 9365 AS id, 'row_9365' AS label;
/* block header 9366 */
SELECT 9367 AS id, 'row_9367' AS label;
/* block header 9368 */
SELECT 9369 AS id, 'row_9369' AS label;
WITH cte_9370 AS (SELECT 9370 AS n) SELECT n FROM cte_9370;
DELETE FROM bench_t_27 WHERE id = 11;
SELECT * FROM "quoted_9372" WHERE col = E'esc\'9372';
SELECT `mysql_9373` FROM `tbl_23`;
$dz$ dollar body 9374 ; semicolon inside $dz$
INSERT INTO bench_t_31 (id, payload) VALUES (9375, 'v9375');
UPDATE bench_t_32 SET payload = 9376 WHERE id = 0;
INSERT INTO bench_t_33 (id, payload) VALUES (9377, 'v9377');
/* block header 9378 */
-- line 9379: deterministic comment
-- line 9380: deterministic comment
SELECT 9381 AS id, 'row_9381' AS label;
BEGIN; SELECT 9382; COMMIT;
# hash comment 9383
SELECT [bracket_9384] FROM [dbo].[tbl_24];
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
WITH cte_9386 AS (SELECT 9386 AS n) SELECT n FROM cte_9386;
DELETE FROM bench_t_11 WHERE id = 11;
BEGIN; SELECT 9388; COMMIT;
SELECT [bracket_9389] FROM [dbo].[tbl_29];
INSERT INTO bench_t_46 (id, payload) VALUES (9390, 'v9390');
SELECT * FROM "quoted_9391" WHERE col = E'esc\'9391';
SELECT 9392 AS id, 'row_9392' AS label;
WITH cte_9393 AS (SELECT 9393 AS n) SELECT n FROM cte_9393;
DELETE FROM bench_t_18 WHERE id = 2;
SELECT nested FROM t WHERE id IN (9395, 9396, 9397);
SELECT [bracket_9396] FROM [dbo].[tbl_36];
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
WITH cte_9398 AS (SELECT 9398 AS n) SELECT n FROM cte_9398;
INSERT INTO bench_t_55 (id, payload) VALUES (9399, 'v9399');
SELECT [bracket_9400] FROM [dbo].[tbl_0];
SELECT [bracket_9401] FROM [dbo].[tbl_1];
SELECT 9402 AS id, 'row_9402' AS label;
SELECT [bracket_9403] FROM [dbo].[tbl_3];
WITH cte_9404 AS (SELECT 9404 AS n) SELECT n FROM cte_9404;
SELECT 9405 AS id, 'row_9405' AS label;
# hash comment 9406
BEGIN; SELECT 9407; COMMIT;
-- line 9408: deterministic comment
UPDATE bench_t_1 SET payload = 9409 WHERE id = 1;
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
$dz$ dollar body 9411 ; semicolon inside $dz$
SELECT 9412 AS id, 'row_9412' AS label;
SELECT `mysql_9413` FROM `tbl_13`;
/* block header 9414 */
SELECT 9415 AS id, 'row_9415' AS label;
SELECT `mysql_9416` FROM `tbl_16`;
# hash comment 9417
SELECT `mysql_9418` FROM `tbl_18`;
$dz$ dollar body 9419 ; semicolon inside $dz$
# hash comment 9420
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
BEGIN; SELECT 9423; COMMIT;
$dz$ dollar body 9424 ; semicolon inside $dz$
BEGIN; SELECT 9425; COMMIT;
# hash comment 9426
-- line 9427: deterministic comment
UPDATE bench_t_20 SET payload = 9428 WHERE id = 20;
SELECT nested FROM t WHERE id IN (9429, 9430, 9431);
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT 9431 AS id, 'row_9431' AS label;
SELECT [bracket_9432] FROM [dbo].[tbl_32];
SELECT 9433 AS id, 'row_9433' AS label;
-- line 9434: deterministic comment
$dz$ dollar body 9435 ; semicolon inside $dz$
DELETE FROM bench_t_28 WHERE id = 12;
UPDATE bench_t_29 SET payload = 9437 WHERE id = 29;
SELECT * FROM "quoted_9438" WHERE col = E'esc\'9438';
$dz$ dollar body 9439 ; semicolon inside $dz$
SELECT `mysql_9440` FROM `tbl_40`;
/* block header 9441 */
DELETE FROM bench_t_2 WHERE id = 2;
$dz$ dollar body 9443 ; semicolon inside $dz$
$dz$ dollar body 9444 ; semicolon inside $dz$
DELETE FROM bench_t_5 WHERE id = 5;
SELECT 9446 AS id, 'row_9446' AS label;
UPDATE bench_t_39 SET payload = 9447 WHERE id = 7;
UPDATE bench_t_40 SET payload = 9448 WHERE id = 8;
SELECT `mysql_9449` FROM `tbl_49`;
# hash comment 9450
DELETE FROM bench_t_11 WHERE id = 11;
# hash comment 9452
BEGIN; SELECT 9453; COMMIT;
INSERT INTO bench_t_110 (id, payload) VALUES (9454, 'v9454');
DELETE FROM bench_t_15 WHERE id = 15;
UPDATE bench_t_48 SET payload = 9456 WHERE id = 16;
-- line 9457: deterministic comment
# hash comment 9458
/* block header 9459 */
SELECT 9460 AS id, 'row_9460' AS label;
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT nested FROM t WHERE id IN (9462, 9463, 9464);
# hash comment 9463
# hash comment 9464
/* block header 9465 */
SELECT 9466 AS id, 'row_9466' AS label;
SELECT 9467 AS id, 'row_9467' AS label;
UPDATE bench_t_60 SET payload = 9468 WHERE id = 28;
INSERT INTO bench_t_125 (id, payload) VALUES (9469, 'v9469');
SELECT 9470 AS id, 'row_9470' AS label;
BEGIN; SELECT 9471; COMMIT;
SELECT * FROM "quoted_9472" WHERE col = E'esc\'9472';
/* block header 9473 */
SELECT `mysql_9474` FROM `tbl_24`;
SELECT [bracket_9475] FROM [dbo].[tbl_35];
BEGIN; SELECT 9476; COMMIT;
# hash comment 9477
SELECT * FROM "quoted_9478" WHERE col = E'esc\'9478';
SELECT 9479 AS id, 'row_9479' AS label;
INSERT INTO bench_t_8 (id, payload) VALUES (9480, 'v9480');
-- line 9481: deterministic comment
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
-- line 9484: deterministic comment
INSERT INTO bench_t_13 (id, payload) VALUES (9485, 'v9485');
$dz$ dollar body 9486 ; semicolon inside $dz$
UPDATE bench_t_15 SET payload = 9487 WHERE id = 15;
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT 9489 AS id, 'row_9489' AS label;
SELECT nested FROM t WHERE id IN (9490, 9491, 9492);
BEGIN; SELECT 9491; COMMIT;
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
-- line 9494: deterministic comment
BEGIN; SELECT 9495; COMMIT;
WITH cte_9496 AS (SELECT 9496 AS n) SELECT n FROM cte_9496;
INSERT INTO bench_t_25 (id, payload) VALUES (9497, 'v9497');
SELECT `mysql_9498` FROM `tbl_48`;
SELECT 9499 AS id, 'row_9499' AS label;
/*
 * section 38
 * checksum cb49
 */
SELECT `mysql_9500` FROM `tbl_0`;
$dz$ dollar body 9505 ; semicolon inside $dz$
# hash comment 9506
$dz$ dollar body 9507 ; semicolon inside $dz$
SELECT 9508 AS id, 'row_9508' AS label;
-- line 9509: deterministic comment
SELECT [bracket_9510] FROM [dbo].[tbl_30];
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
# hash comment 9512
/* block header 9513 */
$dz$ dollar body 9514 ; semicolon inside $dz$
INSERT INTO bench_t_43 (id, payload) VALUES (9515, 'O''Brien');
WITH cte_9516 AS (SELECT 9516 AS n) SELECT n FROM cte_9516;
UPDATE bench_t_45 SET payload = 9517 WHERE id = 13;
SELECT [bracket_9518] FROM [dbo].[tbl_38];
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT 9520 AS id, 'row_9520' AS label;
SELECT 9521 AS id, 'row_9521' AS label;
UPDATE bench_t_50 SET payload = 9522 WHERE id = 18;
$dz$ dollar body 9523 ; semicolon inside $dz$
INSERT INTO bench_t_52 (id, payload) VALUES (9524, 'v9524');
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
/* block header 9526 */
DELETE FROM bench_t_23 WHERE id = 7;
BEGIN; SELECT 9528; COMMIT;
SELECT nested FROM t WHERE id IN (9529, 9530, 9531);
WITH cte_9530 AS (SELECT 9530 AS n) SELECT n FROM cte_9530;
SELECT * FROM "quoted_9531" WHERE col = E'esc\'9531';
SELECT nested FROM t WHERE id IN (9532, 9533, 9534);
SELECT [bracket_9533] FROM [dbo].[tbl_13];
$dz$ dollar body 9534 ; semicolon inside $dz$
# hash comment 9535
SELECT nested FROM t WHERE id IN (9536, 9537, 9538);
# hash comment 9537
-- line 9538: deterministic comment
UPDATE bench_t_3 SET payload = 9539 WHERE id = 3;
SELECT * FROM "quoted_9540" WHERE col = E'esc\'9540';
/* block header 9541 */
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
UPDATE bench_t_7 SET payload = 9543 WHERE id = 7;
INSERT INTO bench_t_72 (id, payload) VALUES (9544, 'v9544');
-- line 9545: deterministic comment
SELECT `mysql_9546` FROM `tbl_46`;
SELECT 9547 AS id, 'row_9547' AS label;
SELECT 9548 AS id, 'row_9548' AS label;
DELETE FROM bench_t_13 WHERE id = 13;
SELECT `mysql_9550` FROM `tbl_0`;
-- line 9551: deterministic comment
SELECT * FROM "quoted_9552" WHERE col = E'esc\'9552';
$dz$ dollar body 9553 ; semicolon inside $dz$
SELECT nested FROM t WHERE id IN (9554, 9555, 9556);
BEGIN; SELECT 9555; COMMIT;
# hash comment 9556
SELECT [bracket_9557] FROM [dbo].[tbl_37];
BEGIN; SELECT 9558; COMMIT;
WITH cte_9559 AS (SELECT 9559 AS n) SELECT n FROM cte_9559;
SELECT * FROM "quoted_9560" WHERE col = E'esc\'9560';
$dz$ dollar body 9561 ; semicolon inside $dz$
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
$dz$ dollar body 9563 ; semicolon inside $dz$
-- line 9564: deterministic comment
SELECT 9565 AS id, 'row_9565' AS label;
SELECT * FROM "quoted_9566" WHERE col = E'esc\'9566';
SELECT 9567 AS id, 'row_9567' AS label;
SELECT nested FROM t WHERE id IN (9568, 9569, 9570);
$dz$ dollar body 9569 ; semicolon inside $dz$
SELECT * FROM "quoted_9570" WHERE col = E'esc\'9570';
$dz$ dollar body 9571 ; semicolon inside $dz$
DELETE FROM bench_t_4 WHERE id = 4;
UPDATE bench_t_37 SET payload = 9573 WHERE id = 5;
WITH cte_9574 AS (SELECT 9574 AS n) SELECT n FROM cte_9574;
SELECT 9575 AS id, 'row_9575' AS label;
WITH cte_9576 AS (SELECT 9576 AS n) SELECT n FROM cte_9576;
/* block header 9577 */
SELECT nested FROM t WHERE id IN (9578, 9579, 9580);
INSERT INTO bench_t_107 (id, payload) VALUES (9579, 'v9579');
WITH cte_9580 AS (SELECT 9580 AS n) SELECT n FROM cte_9580;
SELECT `mysql_9581` FROM `tbl_31`;
# hash comment 9582
SELECT nested FROM t WHERE id IN (9583, 9584, 9585);
SELECT `mysql_9584` FROM `tbl_34`;
SELECT `mysql_9585` FROM `tbl_35`;
# hash comment 9586
# hash comment 9587
UPDATE bench_t_52 SET payload = 9588 WHERE id = 20;
WITH cte_9589 AS (SELECT 9589 AS n) SELECT n FROM cte_9589;
$dz$ dollar body 9590 ; semicolon inside $dz$
# hash comment 9591
-- line 9592: deterministic comment
-- line 9593: deterministic comment
/* block header 9594 */
BEGIN; SELECT 9595; COMMIT;
SELECT [bracket_9596] FROM [dbo].[tbl_36];
-- line 9597: deterministic comment
-- line 9598: deterministic comment
/* block header 9599 */
/* block header 9600 */
BEGIN; SELECT 9601; COMMIT;
BEGIN; SELECT 9602; COMMIT;
/* block header 9603 */
WITH cte_9604 AS (SELECT 9604 AS n) SELECT n FROM cte_9604;
DELETE FROM bench_t_5 WHERE id = 5;
SELECT 9606 AS id, 'row_9606' AS label;
UPDATE bench_t_7 SET payload = 9607 WHERE id = 7;
SELECT nested FROM t WHERE id IN (9608, 9609, 9610);
SELECT [bracket_9609] FROM [dbo].[tbl_9];
SELECT [bracket_9610] FROM [dbo].[tbl_10];
$dz$ dollar body 9611 ; semicolon inside $dz$
DELETE FROM bench_t_12 WHERE id = 12;
INSERT INTO bench_t_13 (id, payload) VALUES (9613, 'v9613');
WITH cte_9614 AS (SELECT 9614 AS n) SELECT n FROM cte_9614;
DELETE FROM bench_t_15 WHERE id = 15;
BEGIN; SELECT 9616; COMMIT;
BEGIN; SELECT 9617; COMMIT;
$dz$ dollar body 9618 ; semicolon inside $dz$
SELECT [bracket_9619] FROM [dbo].[tbl_19];
UPDATE bench_t_20 SET payload = 9620 WHERE id = 20;
# hash comment 9621
BEGIN; SELECT 9622; COMMIT;
DELETE FROM bench_t_23 WHERE id = 7;
WITH cte_9624 AS (SELECT 9624 AS n) SELECT n FROM cte_9624;
SELECT [bracket_9625] FROM [dbo].[tbl_25];
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT `mysql_9627` FROM `tbl_27`;
SELECT [bracket_9628] FROM [dbo].[tbl_28];
BEGIN; SELECT 9629; COMMIT;
WITH cte_9630 AS (SELECT 9630 AS n) SELECT n FROM cte_9630;
WITH cte_9631 AS (SELECT 9631 AS n) SELECT n FROM cte_9631;
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
-- line 9633: deterministic comment
SELECT * FROM "quoted_9634" WHERE col = E'esc\'9634';
SELECT [bracket_9635] FROM [dbo].[tbl_35];
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT `mysql_9637` FROM `tbl_37`;
-- line 9638: deterministic comment
SELECT * FROM "quoted_9639" WHERE col = E'esc\'9639';
SELECT * FROM "quoted_9640" WHERE col = E'esc\'9640';
SELECT `mysql_9641` FROM `tbl_41`;
$dz$ dollar body 9642 ; semicolon inside $dz$
/* block header 9643 */
SELECT nested FROM t WHERE id IN (9644, 9645, 9646);
# hash comment 9645
SELECT 9646 AS id, 'row_9646' AS label;
SELECT `mysql_9647` FROM `tbl_47`;
# hash comment 9648
SELECT * FROM "quoted_9649" WHERE col = E'esc\'9649';
BEGIN; SELECT 9650; COMMIT;
WITH cte_9651 AS (SELECT 9651 AS n) SELECT n FROM cte_9651;
SELECT * FROM "quoted_9652" WHERE col = E'esc\'9652';
/* block header 9653 */
# hash comment 9654
INSERT INTO bench_t_55 (id, payload) VALUES (9655, 'v9655');
SELECT 9656 AS id, 'row_9656' AS label;
-- line 9657: deterministic comment
WITH cte_9658 AS (SELECT 9658 AS n) SELECT n FROM cte_9658;
INSERT INTO bench_t_59 (id, payload) VALUES (9659, 'v9659');
/* block header 9660 */
SELECT * FROM "quoted_9661" WHERE col = E'esc\'9661';
SELECT * FROM "quoted_9662" WHERE col = E'esc\'9662';
SELECT * FROM "quoted_9663" WHERE col = E'esc\'9663';
UPDATE bench_t_0 SET payload = 9664 WHERE id = 0;
# hash comment 9665
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
/* block header 9669 */
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT `mysql_9671` FROM `tbl_21`;
SELECT [bracket_9672] FROM [dbo].[tbl_32];
-- line 9673: deterministic comment
# hash comment 9674
UPDATE bench_t_11 SET payload = 9675 WHERE id = 11;
SELECT * FROM "quoted_9676" WHERE col = E'esc\'9676';
SELECT `mysql_9677` FROM `tbl_27`;
$dz$ dollar body 9678 ; semicolon inside $dz$
/* block header 9679 */
$dz$ dollar body 9680 ; semicolon inside $dz$
UPDATE bench_t_17 SET payload = 9681 WHERE id = 17;
SELECT `mysql_9682` FROM `tbl_32`;
-- line 9683: deterministic comment
# hash comment 9684
-- line 9685: deterministic comment
UPDATE bench_t_22 SET payload = 9686 WHERE id = 22;
BEGIN; SELECT 9687; COMMIT;
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
-- line 9689: deterministic comment
WITH cte_9690 AS (SELECT 9690 AS n) SELECT n FROM cte_9690;
WITH cte_9691 AS (SELECT 9691 AS n) SELECT n FROM cte_9691;
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
$dz$ dollar body 9693 ; semicolon inside $dz$
SELECT 9694 AS id, 'row_9694' AS label;
-- line 9695: deterministic comment
SELECT `mysql_9696` FROM `tbl_46`;
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT nested FROM t WHERE id IN (9698, 9699, 9700);
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
INSERT INTO bench_t_100 (id, payload) VALUES (9700, 'v9700');
INSERT INTO bench_t_101 (id, payload) VALUES (9701, 'v9701');
# hash comment 9702
-- line 9703: deterministic comment
$dz$ dollar body 9704 ; semicolon inside $dz$
$dz$ dollar body 9705 ; semicolon inside $dz$
DELETE FROM bench_t_10 WHERE id = 10;
UPDATE bench_t_43 SET payload = 9707 WHERE id = 11;
-- line 9708: deterministic comment
SELECT 9709 AS id, 'row_9709' AS label;
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
INSERT INTO bench_t_111 (id, payload) VALUES (9711, 'v9711');
SELECT 9712 AS id, 'row_9712' AS label;
/* block header 9713 */
SELECT nested FROM t WHERE id IN (9714, 9715, 9716);
SELECT * FROM "quoted_9715" WHERE col = E'esc\'9715';
INSERT INTO bench_t_116 (id, payload) VALUES (9716, 'v9716');
/* block header 9717 */
SELECT nested FROM t WHERE id IN (9718, 9719, 9720);
SELECT [bracket_9719] FROM [dbo].[tbl_39];
UPDATE bench_t_56 SET payload = 9720 WHERE id = 24;
WITH cte_9721 AS (SELECT 9721 AS n) SELECT n FROM cte_9721;
-- line 9722: deterministic comment
UPDATE bench_t_59 SET payload = 9723 WHERE id = 27;
BEGIN; SELECT 9724; COMMIT;
WITH cte_9725 AS (SELECT 9725 AS n) SELECT n FROM cte_9725;
INSERT INTO bench_t_126 (id, payload) VALUES (9726, 'v9726');
SELECT 9727 AS id, 'row_9727' AS label;
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
BEGIN; SELECT 9729; COMMIT;
WITH cte_9730 AS (SELECT 9730 AS n) SELECT n FROM cte_9730;
BEGIN; SELECT 9731; COMMIT;
$dz$ dollar body 9732 ; semicolon inside $dz$
# hash comment 9733
/* block header 9734 */
# hash comment 9735
SELECT nested FROM t WHERE id IN (9736, 9737, 9738);
SELECT [bracket_9737] FROM [dbo].[tbl_17];
SELECT 9738 AS id, 'row_9738' AS label;
-- line 9739: deterministic comment
SELECT 9740 AS id, 'row_9740' AS label;
SELECT `mysql_9741` FROM `tbl_41`;
WITH cte_9742 AS (SELECT 9742 AS n) SELECT n FROM cte_9742;
SELECT `mysql_9743` FROM `tbl_43`;
SELECT [bracket_9744] FROM [dbo].[tbl_24];
SELECT `mysql_9745` FROM `tbl_45`;
# hash comment 9746
SELECT 9747 AS id, 'row_9747' AS label;
-- line 9748: deterministic comment
$dz$ dollar body 9749 ; semicolon inside $dz$
/*
 * section 39
 * checksum 5df0
 */
DELETE FROM bench_t_22 WHERE id = 6;
-- line 9755: deterministic comment
$dz$ dollar body 9756 ; semicolon inside $dz$
DELETE FROM bench_t_29 WHERE id = 13;
BEGIN; SELECT 9758; COMMIT;
INSERT INTO bench_t_31 (id, payload) VALUES (9759, 'v9759');
$dz$ dollar body 9760 ; semicolon inside $dz$
/* block header 9761 */
/* block header 9762 */
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
BEGIN; SELECT 9764; COMMIT;
INSERT INTO bench_t_37 (id, payload) VALUES (9765, 'v9765');
SELECT nested FROM t WHERE id IN (9766, 9767, 9768);
# hash comment 9767
DELETE FROM bench_t_8 WHERE id = 8;
WITH cte_9769 AS (SELECT 9769 AS n) SELECT n FROM cte_9769;
INSERT INTO bench_t_42 (id, payload) VALUES (9770, 'v9770');
DELETE FROM bench_t_11 WHERE id = 11;
SELECT * FROM "quoted_9772" WHERE col = E'esc\'9772';
/* block header 9773 */
/* block header 9774 */
-- line 9775: deterministic comment
UPDATE bench_t_48 SET payload = 9776 WHERE id = 16;
SELECT * FROM "quoted_9777" WHERE col = E'esc\'9777';
# hash comment 9778
SELECT [bracket_9779] FROM [dbo].[tbl_19];
-- line 9780: deterministic comment
SELECT 9781 AS id, 'row_9781' AS label;
SELECT * FROM "quoted_9782" WHERE col = E'esc\'9782';
DELETE FROM bench_t_23 WHERE id = 7;
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
$dz$ dollar body 9785 ; semicolon inside $dz$
BEGIN; SELECT 9786; COMMIT;
INSERT INTO bench_t_59 (id, payload) VALUES (9787, 'v9787');
SELECT 9788 AS id, 'row_9788' AS label;
SELECT 9789 AS id, 'row_9789' AS label;
SELECT * FROM "quoted_9790" WHERE col = E'esc\'9790';
SELECT * FROM "quoted_9791" WHERE col = E'esc\'9791';
INSERT INTO bench_t_64 (id, payload) VALUES (9792, 'v9792');
-- line 9793: deterministic comment
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT `mysql_9795` FROM `tbl_45`;
SELECT 9796 AS id, 'row_9796' AS label;
UPDATE bench_t_5 SET payload = 9797 WHERE id = 5;
SELECT 9798 AS id, 'row_9798' AS label;
$dz$ dollar body 9799 ; semicolon inside $dz$
WITH cte_9800 AS (SELECT 9800 AS n) SELECT n FROM cte_9800;
SELECT nested FROM t WHERE id IN (9801, 9802, 9803);
/* block header 9802 */
-- line 9803: deterministic comment
SELECT [bracket_9804] FROM [dbo].[tbl_4];
DELETE FROM bench_t_13 WHERE id = 13;
-- line 9806: deterministic comment
SELECT * FROM "quoted_9807" WHERE col = E'esc\'9807';
SELECT nested FROM t WHERE id IN (9808, 9809, 9810);
$dz$ dollar body 9809 ; semicolon inside $dz$
SELECT nested FROM t WHERE id IN (9810, 9811, 9812);
/* block header 9811 */
SELECT `mysql_9812` FROM `tbl_12`;
UPDATE bench_t_21 SET payload = 9813 WHERE id = 21;
BEGIN; SELECT 9814; COMMIT;
# hash comment 9815
SELECT * FROM "quoted_9816" WHERE col = E'esc\'9816';
SELECT nested FROM t WHERE id IN (9817, 9818, 9819);
-- line 9818: deterministic comment
-- line 9819: deterministic comment
BEGIN; SELECT 9820; COMMIT;
SELECT nested FROM t WHERE id IN (9821, 9822, 9823);
INSERT INTO bench_t_94 (id, payload) VALUES (9822, 'v9822');
SELECT `mysql_9823` FROM `tbl_23`;
-- line 9824: deterministic comment
WITH cte_9825 AS (SELECT 9825 AS n) SELECT n FROM cte_9825;
DELETE FROM bench_t_2 WHERE id = 2;
SELECT `mysql_9827` FROM `tbl_27`;
BEGIN; SELECT 9828; COMMIT;
-- line 9829: deterministic comment
$dz$ dollar body 9830 ; semicolon inside $dz$
SELECT 9831 AS id, 'row_9831' AS label;
BEGIN; SELECT 9832; COMMIT;
DELETE FROM bench_t_9 WHERE id = 9;
# hash comment 9834
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT nested FROM t WHERE id IN (9836, 9837, 9838);
SELECT 9837 AS id, 'row_9837' AS label;
SELECT [bracket_9838] FROM [dbo].[tbl_38];
SELECT 9839 AS id, 'row_9839' AS label;
$dz$ dollar body 9840 ; semicolon inside $dz$
# hash comment 9841
SELECT * FROM "quoted_9842" WHERE col = E'esc\'9842';
$dz$ dollar body 9843 ; semicolon inside $dz$
# hash comment 9844
INSERT INTO bench_t_117 (id, payload) VALUES (9845, 'O''Brien');
DELETE FROM bench_t_22 WHERE id = 6;
# hash comment 9847
SELECT `mysql_9848` FROM `tbl_48`;
WITH cte_9849 AS (SELECT 9849 AS n) SELECT n FROM cte_9849;
-- line 9850: deterministic comment
SELECT [bracket_9851] FROM [dbo].[tbl_11];
WITH cte_9852 AS (SELECT 9852 AS n) SELECT n FROM cte_9852;
/* block header 9853 */
UPDATE bench_t_62 SET payload = 9854 WHERE id = 30;
SELECT * FROM "quoted_9855" WHERE col = E'esc\'9855';
SELECT [bracket_9856] FROM [dbo].[tbl_16];
WITH cte_9857 AS (SELECT 9857 AS n) SELECT n FROM cte_9857;
UPDATE bench_t_2 SET payload = 9858 WHERE id = 2;
SELECT `mysql_9859` FROM `tbl_9`;
INSERT INTO bench_t_4 (id, payload) VALUES (9860, 'v9860');
DELETE FROM bench_t_5 WHERE id = 5;
SELECT [bracket_9862] FROM [dbo].[tbl_22];
UPDATE bench_t_7 SET payload = 9863 WHERE id = 7;
$dz$ dollar body 9864 ; semicolon inside $dz$
BEGIN; SELECT 9865; COMMIT;
SELECT nested FROM t WHERE id IN (9866, 9867, 9868);
UPDATE bench_t_11 SET payload = 9867 WHERE id = 11;
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
-- line 9869: deterministic comment
SELECT `mysql_9870` FROM `tbl_20`;
# hash comment 9871
$dz$ dollar body 9872 ; semicolon inside $dz$
UPDATE bench_t_17 SET payload = 9873 WHERE id = 17;
-- line 9874: deterministic comment
SELECT nested FROM t WHERE id IN (9875, 9876, 9877);
# hash comment 9876
WITH cte_9877 AS (SELECT 9877 AS n) SELECT n FROM cte_9877;
WITH cte_9878 AS (SELECT 9878 AS n) SELECT n FROM cte_9878;
UPDATE bench_t_23 SET payload = 9879 WHERE id = 23;
SELECT `mysql_9880` FROM `tbl_30`;
# hash comment 9881
DELETE FROM bench_t_26 WHERE id = 10;
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
/* block header 9884 */
$dz$ dollar body 9885 ; semicolon inside $dz$
$dz$ dollar body 9886 ; semicolon inside $dz$
SELECT `mysql_9887` FROM `tbl_37`;
-- line 9888: deterministic comment
SELECT [bracket_9889] FROM [dbo].[tbl_9];
-- line 9890: deterministic comment
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT [bracket_9892] FROM [dbo].[tbl_12];
UPDATE bench_t_37 SET payload = 9893 WHERE id = 5;
INSERT INTO bench_t_38 (id, payload) VALUES (9894, 'v9894');
WITH cte_9895 AS (SELECT 9895 AS n) SELECT n FROM cte_9895;
SELECT * FROM "quoted_9896" WHERE col = E'esc\'9896';
# hash comment 9897
$dz$ dollar body 9898 ; semicolon inside $dz$
UPDATE bench_t_43 SET payload = 9899 WHERE id = 11;
DELETE FROM bench_t_12 WHERE id = 12;
$dz$ dollar body 9901 ; semicolon inside $dz$
DELETE FROM bench_t_14 WHERE id = 14;
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
WITH cte_9904 AS (SELECT 9904 AS n) SELECT n FROM cte_9904;
-- line 9905: deterministic comment
UPDATE bench_t_50 SET payload = 9906 WHERE id = 18;
SELECT 9907 AS id, 'row_9907' AS label;
SELECT `mysql_9908` FROM `tbl_8`;
UPDATE bench_t_53 SET payload = 9909 WHERE id = 21;
SELECT 9910 AS id, 'row_9910' AS label;
SELECT nested FROM t WHERE id IN (9911, 9912, 9913);
BEGIN; SELECT 9912; COMMIT;
UPDATE bench_t_57 SET payload = 9913 WHERE id = 25;
SELECT `mysql_9914` FROM `tbl_14`;
SELECT * FROM "quoted_9915" WHERE col = E'esc\'9915';
BEGIN; SELECT 9916; COMMIT;
WITH cte_9917 AS (SELECT 9917 AS n) SELECT n FROM cte_9917;
$dz$ dollar body 9918 ; semicolon inside $dz$
BEGIN; SELECT 9919; COMMIT;
SELECT [bracket_9920] FROM [dbo].[tbl_0];
DELETE FROM bench_t_1 WHERE id = 1;
SELECT nested FROM t WHERE id IN (9922, 9923, 9924);
BEGIN; SELECT 9923; COMMIT;
WITH cte_9924 AS (SELECT 9924 AS n) SELECT n FROM cte_9924;
# hash comment 9925
SELECT 9926 AS id, 'row_9926' AS label;
SELECT `mysql_9927` FROM `tbl_27`;
SELECT `mysql_9928` FROM `tbl_28`;
SELECT * FROM "quoted_9929" WHERE col = E'esc\'9929';
$dz$ dollar body 9930 ; semicolon inside $dz$
INSERT INTO bench_t_75 (id, payload) VALUES (9931, 'v9931');
SELECT 9932 AS id, 'row_9932' AS label;
$dz$ dollar body 9933 ; semicolon inside $dz$
INSERT INTO bench_t_78 (id, payload) VALUES (9934, 'v9934');
SELECT nested FROM t WHERE id IN (9935, 9936, 9937);
WITH cte_9936 AS (SELECT 9936 AS n) SELECT n FROM cte_9936;
SELECT nested FROM t WHERE id IN (9937, 9938, 9939);
# hash comment 9938
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT nested FROM t WHERE id IN (9940, 9941, 9942);
-- line 9941: deterministic comment
BEGIN; SELECT 9942; COMMIT;
# hash comment 9943
SELECT 9944 AS id, 'row_9944' AS label;
SELECT * FROM "quoted_9945" WHERE col = E'esc\'9945';
$dz$ dollar body 9946 ; semicolon inside $dz$
INSERT INTO bench_t_91 (id, payload) VALUES (9947, 'v9947');
DELETE FROM bench_t_28 WHERE id = 12;
INSERT INTO bench_t_93 (id, payload) VALUES (9949, 'v9949');
$dz$ dollar body 9950 ; semicolon inside $dz$
DELETE FROM bench_t_31 WHERE id = 15;
-- line 9952: deterministic comment
SELECT 9953 AS id, 'row_9953' AS label;
SELECT 9954 AS id, 'row_9954' AS label;
SELECT 9955 AS id, 'row_9955' AS label;
SELECT `mysql_9956` FROM `tbl_6`;
SELECT 9957 AS id, 'row_9957' AS label;
SELECT nested FROM t WHERE id IN (9958, 9959, 9960);
SELECT `mysql_9959` FROM `tbl_9`;
UPDATE bench_t_40 SET payload = 9960 WHERE id = 8;
SELECT * FROM "quoted_9961" WHERE col = E'esc\'9961';
SELECT nested FROM t WHERE id IN (9962, 9963, 9964);
$dz$ dollar body 9963 ; semicolon inside $dz$
UPDATE bench_t_44 SET payload = 9964 WHERE id = 12;
SELECT * FROM "quoted_9965" WHERE col = E'esc\'9965';
SELECT nested FROM t WHERE id IN (9966, 9967, 9968);
$dz$ dollar body 9967 ; semicolon inside $dz$
/* block header 9968 */
DELETE FROM bench_t_17 WHERE id = 1;
SELECT 9970 AS id, 'row_9970' AS label;
SELECT nested FROM t WHERE id IN (9971, 9972, 9973);
INSERT INTO bench_t_116 (id, payload) VALUES (9972, 'v9972');
SELECT `mysql_9973` FROM `tbl_23`;
# hash comment 9974
-- line 9975: deterministic comment
SELECT 9976 AS id, 'row_9976' AS label;
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
INSERT INTO bench_t_122 (id, payload) VALUES (9978, 'v9978');
BEGIN; SELECT 9979; COMMIT;
BEGIN; SELECT 9980; COMMIT;
SELECT `mysql_9981` FROM `tbl_31`;
SELECT `mysql_9982` FROM `tbl_32`;
UPDATE bench_t_63 SET payload = 9983 WHERE id = 31;
INSERT INTO bench_t_0 (id, payload) VALUES (9984, 'v9984');
SELECT * FROM "quoted_9985" WHERE col = E'esc\'9985';
# hash comment 9986
SELECT `mysql_9987` FROM `tbl_37`;
BEGIN; SELECT 9988; COMMIT;
-- line 9989: deterministic comment
$dz$ dollar body 9990 ; semicolon inside $dz$
INSERT INTO bench_t_7 (id, payload) VALUES (9991, 'v9991');
SELECT [bracket_9992] FROM [dbo].[tbl_32];
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
BEGIN; SELECT 9994; COMMIT;
WITH cte_9995 AS (SELECT 9995 AS n) SELECT n FROM cte_9995;
WITH cte_9996 AS (SELECT 9996 AS n) SELECT n FROM cte_9996;
SELECT nested FROM t WHERE id IN (9997, 9998, 9999);
-- line 9998: deterministic comment
SELECT * FROM "quoted_9999" WHERE col = E'esc\'9999';
/*
 * section 40
 * checksum 59b3
 */
SELECT [bracket_10000] FROM [dbo].[tbl_0];
SELECT [bracket_10005] FROM [dbo].[tbl_5];
# hash comment 10006
$dz$ dollar body 10007 ; semicolon inside $dz$
WITH cte_10008 AS (SELECT 10008 AS n) SELECT n FROM cte_10008;
SELECT [bracket_10009] FROM [dbo].[tbl_9];
BEGIN; SELECT 10010; COMMIT;
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
/* block header 10012 */
SELECT * FROM "quoted_10013" WHERE col = E'esc\'10013';
BEGIN; SELECT 10014; COMMIT;
SELECT 10015 AS id, 'row_10015' AS label;
SELECT `mysql_10016` FROM `tbl_16`;
BEGIN; SELECT 10017; COMMIT;
SELECT `mysql_10018` FROM `tbl_18`;
WITH cte_10019 AS (SELECT 10019 AS n) SELECT n FROM cte_10019;
# hash comment 10020
SELECT * FROM "quoted_10021" WHERE col = E'esc\'10021';
SELECT 10022 AS id, 'row_10022' AS label;
-- line 10023: deterministic comment
# hash comment 10024
DELETE FROM bench_t_9 WHERE id = 9;
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT * FROM "quoted_10027" WHERE col = E'esc\'10027';
WITH cte_10028 AS (SELECT 10028 AS n) SELECT n FROM cte_10028;
SELECT nested FROM t WHERE id IN (10029, 10030, 10031);
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
$dz$ dollar body 10031 ; semicolon inside $dz$
SELECT * FROM "quoted_10032" WHERE col = E'esc\'10032';
WITH cte_10033 AS (SELECT 10033 AS n) SELECT n FROM cte_10033;
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT 10035 AS id, 'row_10035' AS label;
SELECT * FROM "quoted_10036" WHERE col = E'esc\'10036';
SELECT [bracket_10037] FROM [dbo].[tbl_37];
UPDATE bench_t_54 SET payload = 10038 WHERE id = 22;
SELECT * FROM "quoted_10039" WHERE col = E'esc\'10039';
UPDATE bench_t_56 SET payload = 10040 WHERE id = 24;
BEGIN; SELECT 10041; COMMIT;
BEGIN; SELECT 10042; COMMIT;
-- line 10043: deterministic comment
$dz$ dollar body 10044 ; semicolon inside $dz$
SELECT [bracket_10045] FROM [dbo].[tbl_5];
WITH cte_10046 AS (SELECT 10046 AS n) SELECT n FROM cte_10046;
$dz$ dollar body 10047 ; semicolon inside $dz$
SELECT `mysql_10048` FROM `tbl_48`;
INSERT INTO bench_t_65 (id, payload) VALUES (10049, 'v10049');
SELECT nested FROM t WHERE id IN (10050, 10051, 10052);
SELECT nested FROM t WHERE id IN (10051, 10052, 10053);
SELECT `mysql_10052` FROM `tbl_2`;
SELECT nested FROM t WHERE id IN (10053, 10054, 10055);
BEGIN; SELECT 10054; COMMIT;
# hash comment 10055
SELECT `mysql_10056` FROM `tbl_6`;
SELECT * FROM "quoted_10057" WHERE col = E'esc\'10057';
# hash comment 10058
UPDATE bench_t_11 SET payload = 10059 WHERE id = 11;
$dz$ dollar body 10060 ; semicolon inside $dz$
/* block header 10061 */
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
-- line 10063: deterministic comment
BEGIN; SELECT 10064; COMMIT;
BEGIN; SELECT 10065; COMMIT;
SELECT `mysql_10066` FROM `tbl_16`;
# hash comment 10067
BEGIN; SELECT 10068; COMMIT;
SELECT [bracket_10069] FROM [dbo].[tbl_29];
SELECT * FROM "quoted_10070" WHERE col = E'esc\'10070';
SELECT 10071 AS id, 'row_10071' AS label;
INSERT INTO bench_t_88 (id, payload) VALUES (10072, 'v10072');
DELETE FROM bench_t_25 WHERE id = 9;
WITH cte_10074 AS (SELECT 10074 AS n) SELECT n FROM cte_10074;
UPDATE bench_t_27 SET payload = 10075 WHERE id = 27;
WITH cte_10076 AS (SELECT 10076 AS n) SELECT n FROM cte_10076;
$dz$ dollar body 10077 ; semicolon inside $dz$
SELECT `mysql_10078` FROM `tbl_28`;
SELECT 10079 AS id, 'row_10079' AS label;
SELECT [bracket_10080] FROM [dbo].[tbl_0];
SELECT * FROM "quoted_10081" WHERE col = E'esc\'10081';
SELECT [bracket_10082] FROM [dbo].[tbl_2];
-- line 10083: deterministic comment
SELECT `mysql_10084` FROM `tbl_34`;
DELETE FROM bench_t_5 WHERE id = 5;
WITH cte_10086 AS (SELECT 10086 AS n) SELECT n FROM cte_10086;
SELECT `mysql_10087` FROM `tbl_37`;
SELECT 10088 AS id, 'row_10088' AS label;
DELETE FROM bench_t_9 WHERE id = 9;
# hash comment 10090
BEGIN; SELECT 10091; COMMIT;
DELETE FROM bench_t_12 WHERE id = 12;
SELECT `mysql_10093` FROM `tbl_43`;
# hash comment 10094
DELETE FROM bench_t_15 WHERE id = 15;
# hash comment 10096
SELECT * FROM "quoted_10097" WHERE col = E'esc\'10097';
DELETE FROM bench_t_18 WHERE id = 2;
WITH cte_10099 AS (SELECT 10099 AS n) SELECT n FROM cte_10099;
-- line 10100: deterministic comment
# hash comment 10101
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
BEGIN; SELECT 10103; COMMIT;
DELETE FROM bench_t_24 WHERE id = 8;
SELECT 10105 AS id, 'row_10105' AS label;
WITH cte_10106 AS (SELECT 10106 AS n) SELECT n FROM cte_10106;
/* block header 10107 */
# hash comment 10108
BEGIN; SELECT 10109; COMMIT;
SELECT `mysql_10110` FROM `tbl_10`;
SELECT * FROM "quoted_10111" WHERE col = E'esc\'10111';
BEGIN; SELECT 10112; COMMIT;
DELETE FROM bench_t_1 WHERE id = 1;
WITH cte_10114 AS (SELECT 10114 AS n) SELECT n FROM cte_10114;
/* block header 10115 */
SELECT `mysql_10116` FROM `tbl_16`;
# hash comment 10117
INSERT INTO bench_t_6 (id, payload) VALUES (10118, 'v10118');
UPDATE bench_t_7 SET payload = 10119 WHERE id = 7;
SELECT nested FROM t WHERE id IN (10120, 10121, 10122);
SELECT nested FROM t WHERE id IN (10121, 10122, 10123);
# hash comment 10122
SELECT * FROM "quoted_10123" WHERE col = E'esc\'10123';
SELECT nested FROM t WHERE id IN (10124, 10125, 10126);
DELETE FROM bench_t_13 WHERE id = 13;
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
-- line 10128: deterministic comment
SELECT * FROM "quoted_10129" WHERE col = E'esc\'10129';
SELECT * FROM "quoted_10130" WHERE col = E'esc\'10130';
SELECT * FROM "quoted_10131" WHERE col = E'esc\'10131';
-- line 10132: deterministic comment
WITH cte_10133 AS (SELECT 10133 AS n) SELECT n FROM cte_10133;
/* block header 10134 */
$dz$ dollar body 10135 ; semicolon inside $dz$
SELECT `mysql_10136` FROM `tbl_36`;
# hash comment 10137
SELECT 10138 AS id, 'row_10138' AS label;
WITH cte_10139 AS (SELECT 10139 AS n) SELECT n FROM cte_10139;
-- line 10140: deterministic comment
DELETE FROM bench_t_29 WHERE id = 13;
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
UPDATE bench_t_31 SET payload = 10143 WHERE id = 31;
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
DELETE FROM bench_t_1 WHERE id = 1;
SELECT `mysql_10146` FROM `tbl_46`;
/* block header 10147 */
-- line 10148: deterministic comment
DELETE FROM bench_t_5 WHERE id = 5;
-- line 10150: deterministic comment
SELECT [bracket_10151] FROM [dbo].[tbl_31];
SELECT nested FROM t WHERE id IN (10152, 10153, 10154);
BEGIN; SELECT 10153; COMMIT;
SELECT * FROM "quoted_10154" WHERE col = E'esc\'10154';
/* block header 10155 */
SELECT `mysql_10156` FROM `tbl_6`;
SELECT `mysql_10157` FROM `tbl_7`;
UPDATE bench_t_46 SET payload = 10158 WHERE id = 14;
# hash comment 10159
BEGIN; SELECT 10160; COMMIT;
SELECT 10161 AS id, 'row_10161' AS label;
# hash comment 10162
SELECT [bracket_10163] FROM [dbo].[tbl_3];
/* block header 10164 */
INSERT INTO bench_t_53 (id, payload) VALUES (10165, 'v10165');
BEGIN; SELECT 10166; COMMIT;
$dz$ dollar body 10167 ; semicolon inside $dz$
$dz$ dollar body 10168 ; semicolon inside $dz$
INSERT INTO bench_t_57 (id, payload) VALUES (10169, 'v10169');
SELECT [bracket_10170] FROM [dbo].[tbl_10];
INSERT INTO bench_t_59 (id, payload) VALUES (10171, 'v10171');
SELECT * FROM "quoted_10172" WHERE col = E'esc\'10172';
/* block header 10173 */
INSERT INTO bench_t_62 (id, payload) VALUES (10174, 'v10174');
BEGIN; SELECT 10175; COMMIT;
DELETE FROM bench_t_0 WHERE id = 0;
WITH cte_10177 AS (SELECT 10177 AS n) SELECT n FROM cte_10177;
SELECT `mysql_10178` FROM `tbl_28`;
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT * FROM "quoted_10180" WHERE col = E'esc\'10180';
# hash comment 10181
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
$dz$ dollar body 10183 ; semicolon inside $dz$
UPDATE bench_t_8 SET payload = 10184 WHERE id = 8;
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
-- line 10186: deterministic comment
BEGIN; SELECT 10187; COMMIT;
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
/* block header 10189 */
/* block header 10190 */
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
$dz$ dollar body 10192 ; semicolon inside $dz$
/* block header 10193 */
SELECT nested FROM t WHERE id IN (10194, 10195, 10196);
SELECT * FROM "quoted_10195" WHERE col = E'esc\'10195';
UPDATE bench_t_20 SET payload = 10196 WHERE id = 20;
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
# hash comment 10198
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
$dz$ dollar body 10200 ; semicolon inside $dz$
DELETE FROM bench_t_25 WHERE id = 9;
SELECT `mysql_10202` FROM `tbl_2`;
DELETE FROM bench_t_27 WHERE id = 11;
SELECT * FROM "quoted_10204" WHERE col = E'esc\'10204';
BEGIN; SELECT 10205; COMMIT;
# hash comment 10206
-- line 10207: deterministic comment
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
/* block header 10209 */
DELETE FROM bench_t_2 WHERE id = 2;
SELECT [bracket_10211] FROM [dbo].[tbl_11];
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
INSERT INTO bench_t_101 (id, payload) VALUES (10213, 'v10213');
SELECT [bracket_10214] FROM [dbo].[tbl_14];
SELECT 10215 AS id, 'row_10215' AS label;
UPDATE bench_t_40 SET payload = 10216 WHERE id = 8;
$dz$ dollar body 10217 ; semicolon inside $dz$
SELECT [bracket_10218] FROM [dbo].[tbl_18];
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT * FROM "quoted_10220" WHERE col = E'esc\'10220';
-- line 10221: deterministic comment
SELECT nested FROM t WHERE id IN (10222, 10223, 10224);
SELECT `mysql_10223` FROM `tbl_23`;
SELECT [bracket_10224] FROM [dbo].[tbl_24];
-- line 10225: deterministic comment
BEGIN; SELECT 10226; COMMIT;
UPDATE bench_t_51 SET payload = 10227 WHERE id = 19;
INSERT INTO bench_t_116 (id, payload) VALUES (10228, 'v10228');
UPDATE bench_t_53 SET payload = 10229 WHERE id = 21;
SELECT nested FROM t WHERE id IN (10230, 10231, 10232);
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
DELETE FROM bench_t_24 WHERE id = 8;
SELECT * FROM "quoted_10233" WHERE col = E'esc\'10233';
SELECT nested FROM t WHERE id IN (10234, 10235, 10236);
-- line 10235: deterministic comment
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT 10237 AS id, 'row_10237' AS label;
UPDATE bench_t_62 SET payload = 10238 WHERE id = 30;
-- line 10239: deterministic comment
SELECT * FROM "quoted_10240" WHERE col = E'esc\'10240';
UPDATE bench_t_1 SET payload = 10241 WHERE id = 1;
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT nested FROM t WHERE id IN (10243, 10244, 10245);
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
DELETE FROM bench_t_5 WHERE id = 5;
-- line 10246: deterministic comment
DELETE FROM bench_t_7 WHERE id = 7;
-- line 10248: deterministic comment
SELECT 10249 AS id, 'row_10249' AS label;
/*
 * section 41
 * checksum ed3f
 */
SELECT [bracket_10250] FROM [dbo].[tbl_10];
SELECT [bracket_10255] FROM [dbo].[tbl_15];
UPDATE bench_t_16 SET payload = 10256 WHERE id = 16;
SELECT nested FROM t WHERE id IN (10257, 10258, 10259);
UPDATE bench_t_18 SET payload = 10258 WHERE id = 18;
WITH cte_10259 AS (SELECT 10259 AS n) SELECT n FROM cte_10259;
SELECT nested FROM t WHERE id IN (10260, 10261, 10262);
$dz$ dollar body 10261 ; semicolon inside $dz$
INSERT INTO bench_t_22 (id, payload) VALUES (10262, 'v10262');
UPDATE bench_t_23 SET payload = 10263 WHERE id = 23;
-- line 10264: deterministic comment
$dz$ dollar body 10265 ; semicolon inside $dz$
SELECT * FROM "quoted_10266" WHERE col = E'esc\'10266';
# hash comment 10267
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT `mysql_10269` FROM `tbl_19`;
WITH cte_10270 AS (SELECT 10270 AS n) SELECT n FROM cte_10270;
UPDATE bench_t_31 SET payload = 10271 WHERE id = 31;
SELECT 10272 AS id, 'row_10272' AS label;
/* block header 10273 */
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
DELETE FROM bench_t_3 WHERE id = 3;
SELECT * FROM "quoted_10276" WHERE col = E'esc\'10276';
$dz$ dollar body 10277 ; semicolon inside $dz$
BEGIN; SELECT 10278; COMMIT;
/* block header 10279 */
BEGIN; SELECT 10280; COMMIT;
BEGIN; SELECT 10281; COMMIT;
INSERT INTO bench_t_42 (id, payload) VALUES (10282, 'v10282');
WITH cte_10283 AS (SELECT 10283 AS n) SELECT n FROM cte_10283;
SELECT nested FROM t WHERE id IN (10284, 10285, 10286);
UPDATE bench_t_45 SET payload = 10285 WHERE id = 13;
SELECT `mysql_10286` FROM `tbl_36`;
$dz$ dollar body 10287 ; semicolon inside $dz$
$dz$ dollar body 10288 ; semicolon inside $dz$
SELECT [bracket_10289] FROM [dbo].[tbl_9];
INSERT INTO bench_t_50 (id, payload) VALUES (10290, 'v10290');
SELECT [bracket_10291] FROM [dbo].[tbl_11];
$dz$ dollar body 10292 ; semicolon inside $dz$
$dz$ dollar body 10293 ; semicolon inside $dz$
-- line 10294: deterministic comment
WITH cte_10295 AS (SELECT 10295 AS n) SELECT n FROM cte_10295;
SELECT nested FROM t WHERE id IN (10296, 10297, 10298);
SELECT nested FROM t WHERE id IN (10297, 10298, 10299);
WITH cte_10298 AS (SELECT 10298 AS n) SELECT n FROM cte_10298;
BEGIN; SELECT 10299; COMMIT;
SELECT 10300 AS id, 'row_10300' AS label;
SELECT [bracket_10301] FROM [dbo].[tbl_21];
# hash comment 10302
DELETE FROM bench_t_31 WHERE id = 15;
-- line 10304: deterministic comment
SELECT 10305 AS id, 'row_10305' AS label;
# hash comment 10306
SELECT 10307 AS id, 'row_10307' AS label;
SELECT [bracket_10308] FROM [dbo].[tbl_28];
BEGIN; SELECT 10309; COMMIT;
# hash comment 10310
/* block header 10311 */
SELECT nested FROM t WHERE id IN (10312, 10313, 10314);
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
UPDATE bench_t_10 SET payload = 10314 WHERE id = 10;
SELECT `mysql_10315` FROM `tbl_15`;
BEGIN; SELECT 10316; COMMIT;
$dz$ dollar body 10317 ; semicolon inside $dz$
/* block header 10318 */
-- line 10319: deterministic comment
BEGIN; SELECT 10320; COMMIT;
-- line 10321: deterministic comment
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
INSERT INTO bench_t_83 (id, payload) VALUES (10323, 'v10323');
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
# hash comment 10325
SELECT * FROM "quoted_10326" WHERE col = E'esc\'10326';
SELECT [bracket_10327] FROM [dbo].[tbl_7];
SELECT nested FROM t WHERE id IN (10328, 10329, 10330);
SELECT nested FROM t WHERE id IN (10329, 10330, 10331);
$dz$ dollar body 10330 ; semicolon inside $dz$
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT 10332 AS id, 'row_10332' AS label;
-- line 10333: deterministic comment
SELECT `mysql_10334` FROM `tbl_34`;
SELECT * FROM "quoted_10335" WHERE col = E'esc\'10335';
SELECT `mysql_10336` FROM `tbl_36`;
DELETE FROM bench_t_1 WHERE id = 1;
INSERT INTO bench_t_98 (id, payload) VALUES (10338, 'v10338');
WITH cte_10339 AS (SELECT 10339 AS n) SELECT n FROM cte_10339;
INSERT INTO bench_t_100 (id, payload) VALUES (10340, 'O''Brien');
SELECT nested FROM t WHERE id IN (10341, 10342, 10343);
UPDATE bench_t_38 SET payload = 10342 WHERE id = 6;
BEGIN; SELECT 10343; COMMIT;
# hash comment 10344
SELECT 10345 AS id, 'row_10345' AS label;
SELECT * FROM "quoted_10346" WHERE col = E'esc\'10346';
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT [bracket_10348] FROM [dbo].[tbl_28];
SELECT 10349 AS id, 'row_10349' AS label;
SELECT `mysql_10350` FROM `tbl_0`;
# hash comment 10351
SELECT nested FROM t WHERE id IN (10352, 10353, 10354);
SELECT * FROM "quoted_10353" WHERE col = E'esc\'10353';
-- line 10354: deterministic comment
SELECT 10355 AS id, 'row_10355' AS label;
WITH cte_10356 AS (SELECT 10356 AS n) SELECT n FROM cte_10356;
SELECT 10357 AS id, 'row_10357' AS label;
BEGIN; SELECT 10358; COMMIT;
SELECT `mysql_10359` FROM `tbl_9`;
-- line 10360: deterministic comment
UPDATE bench_t_57 SET payload = 10361 WHERE id = 25;
INSERT INTO bench_t_122 (id, payload) VALUES (10362, 'O''Brien');
/* block header 10363 */
SELECT nested FROM t WHERE id IN (10364, 10365, 10366);
SELECT `mysql_10365` FROM `tbl_15`;
BEGIN; SELECT 10366; COMMIT;
INSERT INTO bench_t_127 (id, payload) VALUES (10367, 'v10367');
SELECT * FROM "quoted_10368" WHERE col = E'esc\'10368';
DELETE FROM bench_t_1 WHERE id = 1;
$dz$ dollar body 10370 ; semicolon inside $dz$
WITH cte_10371 AS (SELECT 10371 AS n) SELECT n FROM cte_10371;
SELECT nested FROM t WHERE id IN (10372, 10373, 10374);
# hash comment 10373
-- line 10374: deterministic comment
SELECT * FROM "quoted_10375" WHERE col = E'esc\'10375';
SELECT `mysql_10376` FROM `tbl_26`;
SELECT 10377 AS id, 'row_10377' AS label;
SELECT * FROM "quoted_10378" WHERE col = E'esc\'10378';
SELECT [bracket_10379] FROM [dbo].[tbl_19];
BEGIN; SELECT 10380; COMMIT;
# hash comment 10381
/* block header 10382 */
SELECT 10383 AS id, 'row_10383' AS label;
UPDATE bench_t_16 SET payload = 10384 WHERE id = 16;
# hash comment 10385
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT * FROM "quoted_10387" WHERE col = E'esc\'10387';
INSERT INTO bench_t_20 (id, payload) VALUES (10388, 'v10388');
WITH cte_10389 AS (SELECT 10389 AS n) SELECT n FROM cte_10389;
-- line 10390: deterministic comment
SELECT 10391 AS id, 'row_10391' AS label;
SELECT 10392 AS id, 'row_10392' AS label;
/* block header 10393 */
WITH cte_10394 AS (SELECT 10394 AS n) SELECT n FROM cte_10394;
INSERT INTO bench_t_27 (id, payload) VALUES (10395, 'O''Brien');
$dz$ dollar body 10396 ; semicolon inside $dz$
INSERT INTO bench_t_29 (id, payload) VALUES (10397, 'v10397');
SELECT nested FROM t WHERE id IN (10398, 10399, 10400);
WITH cte_10399 AS (SELECT 10399 AS n) SELECT n FROM cte_10399;
UPDATE bench_t_32 SET payload = 10400 WHERE id = 0;
UPDATE bench_t_33 SET payload = 10401 WHERE id = 1;
DELETE FROM bench_t_2 WHERE id = 2;
/* block header 10403 */
$dz$ dollar body 10404 ; semicolon inside $dz$
SELECT 10405 AS id, 'row_10405' AS label;
WITH cte_10406 AS (SELECT 10406 AS n) SELECT n FROM cte_10406;
/* block header 10407 */
DELETE FROM bench_t_8 WHERE id = 8;
/* block header 10409 */
UPDATE bench_t_42 SET payload = 10410 WHERE id = 10;
UPDATE bench_t_43 SET payload = 10411 WHERE id = 11;
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
WITH cte_10413 AS (SELECT 10413 AS n) SELECT n FROM cte_10413;
/* block header 10414 */
/* block header 10415 */
WITH cte_10416 AS (SELECT 10416 AS n) SELECT n FROM cte_10416;
-- line 10417: deterministic comment
INSERT INTO bench_t_50 (id, payload) VALUES (10418, 'v10418');
$dz$ dollar body 10419 ; semicolon inside $dz$
/* block header 10420 */
# hash comment 10421
$dz$ dollar body 10422 ; semicolon inside $dz$
$dz$ dollar body 10423 ; semicolon inside $dz$
SELECT 10424 AS id, 'row_10424' AS label;
SELECT nested FROM t WHERE id IN (10425, 10426, 10427);
UPDATE bench_t_58 SET payload = 10426 WHERE id = 26;
BEGIN; SELECT 10427; COMMIT;
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
BEGIN; SELECT 10429; COMMIT;
/* block header 10430 */
UPDATE bench_t_63 SET payload = 10431 WHERE id = 31;
-- line 10432: deterministic comment
INSERT INTO bench_t_65 (id, payload) VALUES (10433, 'v10433');
/* block header 10434 */
$dz$ dollar body 10435 ; semicolon inside $dz$
/* block header 10436 */
SELECT * FROM "quoted_10437" WHERE col = E'esc\'10437';
SELECT `mysql_10438` FROM `tbl_38`;
WITH cte_10439 AS (SELECT 10439 AS n) SELECT n FROM cte_10439;
/* block header 10440 */
INSERT INTO bench_t_73 (id, payload) VALUES (10441, 'v10441');
-- line 10442: deterministic comment
INSERT INTO bench_t_75 (id, payload) VALUES (10443, 'v10443');
WITH cte_10444 AS (SELECT 10444 AS n) SELECT n FROM cte_10444;
-- line 10445: deterministic comment
SELECT nested FROM t WHERE id IN (10446, 10447, 10448);
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
BEGIN; SELECT 10448; COMMIT;
SELECT nested FROM t WHERE id IN (10449, 10450, 10451);
UPDATE bench_t_18 SET payload = 10450 WHERE id = 18;
SELECT nested FROM t WHERE id IN (10451, 10452, 10453);
# hash comment 10452
SELECT nested FROM t WHERE id IN (10453, 10454, 10455);
BEGIN; SELECT 10454; COMMIT;
SELECT 10455 AS id, 'row_10455' AS label;
$dz$ dollar body 10456 ; semicolon inside $dz$
DELETE FROM bench_t_25 WHERE id = 9;
UPDATE bench_t_26 SET payload = 10458 WHERE id = 26;
UPDATE bench_t_27 SET payload = 10459 WHERE id = 27;
SELECT nested FROM t WHERE id IN (10460, 10461, 10462);
DELETE FROM bench_t_29 WHERE id = 13;
SELECT 10462 AS id, 'row_10462' AS label;
SELECT * FROM "quoted_10463" WHERE col = E'esc\'10463';
BEGIN; SELECT 10464; COMMIT;
SELECT 10465 AS id, 'row_10465' AS label;
SELECT `mysql_10466` FROM `tbl_16`;
UPDATE bench_t_35 SET payload = 10467 WHERE id = 3;
SELECT 10468 AS id, 'row_10468' AS label;
SELECT * FROM "quoted_10469" WHERE col = E'esc\'10469';
$dz$ dollar body 10470 ; semicolon inside $dz$
SELECT 10471 AS id, 'row_10471' AS label;
UPDATE bench_t_40 SET payload = 10472 WHERE id = 8;
SELECT [bracket_10473] FROM [dbo].[tbl_33];
WITH cte_10474 AS (SELECT 10474 AS n) SELECT n FROM cte_10474;
SELECT [bracket_10475] FROM [dbo].[tbl_35];
# hash comment 10476
BEGIN; SELECT 10477; COMMIT;
-- line 10478: deterministic comment
SELECT nested FROM t WHERE id IN (10479, 10480, 10481);
UPDATE bench_t_48 SET payload = 10480 WHERE id = 16;
-- line 10481: deterministic comment
SELECT * FROM "quoted_10482" WHERE col = E'esc\'10482';
# hash comment 10483
DELETE FROM bench_t_20 WHERE id = 4;
SELECT `mysql_10485` FROM `tbl_35`;
# hash comment 10486
DELETE FROM bench_t_23 WHERE id = 7;
$dz$ dollar body 10488 ; semicolon inside $dz$
WITH cte_10489 AS (SELECT 10489 AS n) SELECT n FROM cte_10489;
SELECT 10490 AS id, 'row_10490' AS label;
SELECT * FROM "quoted_10491" WHERE col = E'esc\'10491';
# hash comment 10492
SELECT `mysql_10493` FROM `tbl_43`;
UPDATE bench_t_62 SET payload = 10494 WHERE id = 30;
BEGIN; SELECT 10495; COMMIT;
# hash comment 10496
SELECT nested FROM t WHERE id IN (10497, 10498, 10499);
INSERT INTO bench_t_2 (id, payload) VALUES (10498, 'v10498');
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
/*
 * section 42
 * checksum be22
 */
SELECT `mysql_10500` FROM `tbl_0`;
/* block header 10505 */
UPDATE bench_t_10 SET payload = 10506 WHERE id = 10;
DELETE FROM bench_t_11 WHERE id = 11;
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT [bracket_10509] FROM [dbo].[tbl_29];
WITH cte_10510 AS (SELECT 10510 AS n) SELECT n FROM cte_10510;
INSERT INTO bench_t_15 (id, payload) VALUES (10511, 'v10511');
/* block header 10512 */
SELECT [bracket_10513] FROM [dbo].[tbl_33];
$dz$ dollar body 10514 ; semicolon inside $dz$
$dz$ dollar body 10515 ; semicolon inside $dz$
-- line 10516: deterministic comment
SELECT [bracket_10517] FROM [dbo].[tbl_37];
SELECT `mysql_10518` FROM `tbl_18`;
SELECT `mysql_10519` FROM `tbl_19`;
UPDATE bench_t_24 SET payload = 10520 WHERE id = 24;
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
BEGIN; SELECT 10522; COMMIT;
WITH cte_10523 AS (SELECT 10523 AS n) SELECT n FROM cte_10523;
SELECT `mysql_10524` FROM `tbl_24`;
/* block header 10525 */
DELETE FROM bench_t_30 WHERE id = 14;
$dz$ dollar body 10527 ; semicolon inside $dz$
/* block header 10528 */
WITH cte_10529 AS (SELECT 10529 AS n) SELECT n FROM cte_10529;
BEGIN; SELECT 10530; COMMIT;
BEGIN; SELECT 10531; COMMIT;
INSERT INTO bench_t_36 (id, payload) VALUES (10532, 'v10532');
-- line 10533: deterministic comment
-- line 10534: deterministic comment
DELETE FROM bench_t_7 WHERE id = 7;
/* block header 10536 */
# hash comment 10537
/* block header 10538 */
WITH cte_10539 AS (SELECT 10539 AS n) SELECT n FROM cte_10539;
BEGIN; SELECT 10540; COMMIT;
# hash comment 10541
SELECT nested FROM t WHERE id IN (10542, 10543, 10544);
UPDATE bench_t_47 SET payload = 10543 WHERE id = 15;
SELECT 10544 AS id, 'row_10544' AS label;
SELECT nested FROM t WHERE id IN (10545, 10546, 10547);
SELECT `mysql_10546` FROM `tbl_46`;
SELECT 10547 AS id, 'row_10547' AS label;
UPDATE bench_t_52 SET payload = 10548 WHERE id = 20;
DELETE FROM bench_t_21 WHERE id = 5;
$dz$ dollar body 10550 ; semicolon inside $dz$
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT 10552 AS id, 'row_10552' AS label;
SELECT * FROM "quoted_10553" WHERE col = E'esc\'10553';
UPDATE bench_t_58 SET payload = 10554 WHERE id = 26;
SELECT `mysql_10555` FROM `tbl_5`;
# hash comment 10556
SELECT 10557 AS id, 'row_10557' AS label;
/* block header 10558 */
SELECT [bracket_10559] FROM [dbo].[tbl_39];
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT `mysql_10561` FROM `tbl_11`;
# hash comment 10562
SELECT 10563 AS id, 'row_10563' AS label;
# hash comment 10564
SELECT nested FROM t WHERE id IN (10565, 10566, 10567);
SELECT nested FROM t WHERE id IN (10566, 10567, 10568);
/* block header 10567 */
SELECT 10568 AS id, 'row_10568' AS label;
DELETE FROM bench_t_9 WHERE id = 9;
BEGIN; SELECT 10570; COMMIT;
WITH cte_10571 AS (SELECT 10571 AS n) SELECT n FROM cte_10571;
SELECT `mysql_10572` FROM `tbl_22`;
SELECT nested FROM t WHERE id IN (10573, 10574, 10575);
SELECT `mysql_10574` FROM `tbl_24`;
WITH cte_10575 AS (SELECT 10575 AS n) SELECT n FROM cte_10575;
$dz$ dollar body 10576 ; semicolon inside $dz$
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
INSERT INTO bench_t_82 (id, payload) VALUES (10578, 'v10578');
SELECT * FROM "quoted_10579" WHERE col = E'esc\'10579';
UPDATE bench_t_20 SET payload = 10580 WHERE id = 20;
BEGIN; SELECT 10581; COMMIT;
INSERT INTO bench_t_86 (id, payload) VALUES (10582, 'O''Brien');
DELETE FROM bench_t_23 WHERE id = 7;
SELECT nested FROM t WHERE id IN (10584, 10585, 10586);
-- line 10585: deterministic comment
SELECT `mysql_10586` FROM `tbl_36`;
$dz$ dollar body 10587 ; semicolon inside $dz$
SELECT 10588 AS id, 'row_10588' AS label;
SELECT * FROM "quoted_10589" WHERE col = E'esc\'10589';
BEGIN; SELECT 10590; COMMIT;
BEGIN; SELECT 10591; COMMIT;
DELETE FROM bench_t_0 WHERE id = 0;
DELETE FROM bench_t_1 WHERE id = 1;
/* block header 10594 */
UPDATE bench_t_35 SET payload = 10595 WHERE id = 3;
/* block header 10596 */
WITH cte_10597 AS (SELECT 10597 AS n) SELECT n FROM cte_10597;
SELECT * FROM "quoted_10598" WHERE col = E'esc\'10598';
UPDATE bench_t_39 SET payload = 10599 WHERE id = 7;
-- line 10600: deterministic comment
/* block header 10601 */
SELECT 10602 AS id, 'row_10602' AS label;
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
UPDATE bench_t_44 SET payload = 10604 WHERE id = 12;
SELECT 10605 AS id, 'row_10605' AS label;
BEGIN; SELECT 10606; COMMIT;
SELECT 10607 AS id, 'row_10607' AS label;
SELECT `mysql_10608` FROM `tbl_8`;
-- line 10609: deterministic comment
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
/* block header 10612 */
SELECT 10613 AS id, 'row_10613' AS label;
SELECT 10614 AS id, 'row_10614' AS label;
SELECT 10615 AS id, 'row_10615' AS label;
# hash comment 10616
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
UPDATE bench_t_58 SET payload = 10618 WHERE id = 26;
WITH cte_10619 AS (SELECT 10619 AS n) SELECT n FROM cte_10619;
SELECT nested FROM t WHERE id IN (10620, 10621, 10622);
BEGIN; SELECT 10621; COMMIT;
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT `mysql_10623` FROM `tbl_23`;
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
INSERT INTO bench_t_1 (id, payload) VALUES (10625, 'v10625');
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
INSERT INTO bench_t_3 (id, payload) VALUES (10627, 'v10627');
# hash comment 10628
$dz$ dollar body 10629 ; semicolon inside $dz$
SELECT * FROM "quoted_10630" WHERE col = E'esc\'10630';
WITH cte_10631 AS (SELECT 10631 AS n) SELECT n FROM cte_10631;
SELECT * FROM "quoted_10632" WHERE col = E'esc\'10632';
WITH cte_10633 AS (SELECT 10633 AS n) SELECT n FROM cte_10633;
SELECT [bracket_10634] FROM [dbo].[tbl_34];
SELECT 10635 AS id, 'row_10635' AS label;
SELECT * FROM "quoted_10636" WHERE col = E'esc\'10636';
BEGIN; SELECT 10637; COMMIT;
SELECT `mysql_10638` FROM `tbl_38`;
SELECT [bracket_10639] FROM [dbo].[tbl_39];
BEGIN; SELECT 10640; COMMIT;
SELECT 10641 AS id, 'row_10641' AS label;
SELECT `mysql_10642` FROM `tbl_42`;
WITH cte_10643 AS (SELECT 10643 AS n) SELECT n FROM cte_10643;
SELECT 10644 AS id, 'row_10644' AS label;
SELECT 10645 AS id, 'row_10645' AS label;
DELETE FROM bench_t_22 WHERE id = 6;
WITH cte_10647 AS (SELECT 10647 AS n) SELECT n FROM cte_10647;
/* block header 10648 */
$dz$ dollar body 10649 ; semicolon inside $dz$
SELECT `mysql_10650` FROM `tbl_0`;
INSERT INTO bench_t_27 (id, payload) VALUES (10651, 'v10651');
SELECT [bracket_10652] FROM [dbo].[tbl_12];
SELECT * FROM "quoted_10653" WHERE col = E'esc\'10653';
# hash comment 10654
-- line 10655: deterministic comment
INSERT INTO bench_t_32 (id, payload) VALUES (10656, 'v10656');
UPDATE bench_t_33 SET payload = 10657 WHERE id = 1;
SELECT nested FROM t WHERE id IN (10658, 10659, 10660);
SELECT nested FROM t WHERE id IN (10659, 10660, 10661);
BEGIN; SELECT 10660; COMMIT;
$dz$ dollar body 10661 ; semicolon inside $dz$
SELECT nested FROM t WHERE id IN (10662, 10663, 10664);
SELECT 10663 AS id, 'row_10663' AS label;
SELECT nested FROM t WHERE id IN (10664, 10665, 10666);
SELECT nested FROM t WHERE id IN (10665, 10666, 10667);
-- line 10666: deterministic comment
BEGIN; SELECT 10667; COMMIT;
WITH cte_10668 AS (SELECT 10668 AS n) SELECT n FROM cte_10668;
SELECT 10669 AS id, 'row_10669' AS label;
SELECT [bracket_10670] FROM [dbo].[tbl_30];
UPDATE bench_t_47 SET payload = 10671 WHERE id = 15;
SELECT nested FROM t WHERE id IN (10672, 10673, 10674);
$dz$ dollar body 10673 ; semicolon inside $dz$
# hash comment 10674
/* block header 10675 */
-- line 10676: deterministic comment
BEGIN; SELECT 10677; COMMIT;
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
DELETE FROM bench_t_23 WHERE id = 7;
/* block header 10680 */
DELETE FROM bench_t_25 WHERE id = 9;
DELETE FROM bench_t_26 WHERE id = 10;
$dz$ dollar body 10683 ; semicolon inside $dz$
SELECT [bracket_10684] FROM [dbo].[tbl_4];
WITH cte_10685 AS (SELECT 10685 AS n) SELECT n FROM cte_10685;
UPDATE bench_t_62 SET payload = 10686 WHERE id = 30;
$dz$ dollar body 10687 ; semicolon inside $dz$
# hash comment 10688
/* block header 10689 */
SELECT nested FROM t WHERE id IN (10690, 10691, 10692);
BEGIN; SELECT 10691; COMMIT;
SELECT nested FROM t WHERE id IN (10692, 10693, 10694);
-- line 10693: deterministic comment
WITH cte_10694 AS (SELECT 10694 AS n) SELECT n FROM cte_10694;
SELECT nested FROM t WHERE id IN (10695, 10696, 10697);
/* block header 10696 */
SELECT nested FROM t WHERE id IN (10697, 10698, 10699);
SELECT 10698 AS id, 'row_10698' AS label;
$dz$ dollar body 10699 ; semicolon inside $dz$
SELECT nested FROM t WHERE id IN (10700, 10701, 10702);
SELECT 10701 AS id, 'row_10701' AS label;
$dz$ dollar body 10702 ; semicolon inside $dz$
$dz$ dollar body 10703 ; semicolon inside $dz$
BEGIN; SELECT 10704; COMMIT;
SELECT [bracket_10705] FROM [dbo].[tbl_25];
SELECT [bracket_10706] FROM [dbo].[tbl_26];
# hash comment 10707
SELECT nested FROM t WHERE id IN (10708, 10709, 10710);
-- line 10709: deterministic comment
SELECT `mysql_10710` FROM `tbl_10`;
/* block header 10711 */
SELECT `mysql_10712` FROM `tbl_12`;
WITH cte_10713 AS (SELECT 10713 AS n) SELECT n FROM cte_10713;
INSERT INTO bench_t_90 (id, payload) VALUES (10714, 'O''Brien');
DELETE FROM bench_t_27 WHERE id = 11;
# hash comment 10716
WITH cte_10717 AS (SELECT 10717 AS n) SELECT n FROM cte_10717;
INSERT INTO bench_t_94 (id, payload) VALUES (10718, 'v10718');
SELECT * FROM "quoted_10719" WHERE col = E'esc\'10719';
SELECT 10720 AS id, 'row_10720' AS label;
WITH cte_10721 AS (SELECT 10721 AS n) SELECT n FROM cte_10721;
SELECT * FROM "quoted_10722" WHERE col = E'esc\'10722';
UPDATE bench_t_35 SET payload = 10723 WHERE id = 3;
$dz$ dollar body 10724 ; semicolon inside $dz$
SELECT nested FROM t WHERE id IN (10725, 10726, 10727);
-- line 10726: deterministic comment
INSERT INTO bench_t_103 (id, payload) VALUES (10727, 'v10727');
-- line 10728: deterministic comment
SELECT `mysql_10729` FROM `tbl_29`;
SELECT 10730 AS id, 'row_10730' AS label;
/* block header 10731 */
SELECT [bracket_10732] FROM [dbo].[tbl_12];
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
BEGIN; SELECT 10734; COMMIT;
INSERT INTO bench_t_111 (id, payload) VALUES (10735, 'v10735');
INSERT INTO bench_t_112 (id, payload) VALUES (10736, 'O''Brien');
DELETE FROM bench_t_17 WHERE id = 1;
# hash comment 10738
SELECT [bracket_10739] FROM [dbo].[tbl_19];
SELECT `mysql_10740` FROM `tbl_40`;
SELECT * FROM "quoted_10741" WHERE col = E'esc\'10741';
UPDATE bench_t_54 SET payload = 10742 WHERE id = 22;
# hash comment 10743
# hash comment 10744
BEGIN; SELECT 10745; COMMIT;
SELECT nested FROM t WHERE id IN (10746, 10747, 10748);
# hash comment 10747
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT `mysql_10749` FROM `tbl_49`;
/*
 * section 43
 * checksum e2c2
 */
# hash comment 10750
UPDATE bench_t_3 SET payload = 10755 WHERE id = 3;
SELECT 10756 AS id, 'row_10756' AS label;
SELECT * FROM "quoted_10757" WHERE col = E'esc\'10757';
DELETE FROM bench_t_6 WHERE id = 6;
UPDATE bench_t_7 SET payload = 10759 WHERE id = 7;
# hash comment 10760
SELECT [bracket_10761] FROM [dbo].[tbl_1];
SELECT nested FROM t WHERE id IN (10762, 10763, 10764);
SELECT * FROM "quoted_10763" WHERE col = E'esc\'10763';
/* block header 10764 */
SELECT 10765 AS id, 'row_10765' AS label;
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT * FROM "quoted_10767" WHERE col = E'esc\'10767';
# hash comment 10768
SELECT 10769 AS id, 'row_10769' AS label;
# hash comment 10770
$dz$ dollar body 10771 ; semicolon inside $dz$
SELECT `mysql_10772` FROM `tbl_22`;
SELECT nested FROM t WHERE id IN (10773, 10774, 10775);
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT [bracket_10775] FROM [dbo].[tbl_15];
# hash comment 10776
SELECT * FROM "quoted_10777" WHERE col = E'esc\'10777';
WITH cte_10778 AS (SELECT 10778 AS n) SELECT n FROM cte_10778;
SELECT nested FROM t WHERE id IN (10779, 10780, 10781);
SELECT [bracket_10780] FROM [dbo].[tbl_20];
SELECT 10781 AS id, 'row_10781' AS label;
DELETE FROM bench_t_30 WHERE id = 14;
-- line 10783: deterministic comment
/* block header 10784 */
DELETE FROM bench_t_1 WHERE id = 1;
-- line 10786: deterministic comment
SELECT `mysql_10787` FROM `tbl_37`;
-- line 10788: deterministic comment
/* block header 10789 */
BEGIN; SELECT 10790; COMMIT;
$dz$ dollar body 10791 ; semicolon inside $dz$
SELECT 10792 AS id, 'row_10792' AS label;
$dz$ dollar body 10793 ; semicolon inside $dz$
SELECT * FROM "quoted_10794" WHERE col = E'esc\'10794';
$dz$ dollar body 10795 ; semicolon inside $dz$
SELECT [bracket_10796] FROM [dbo].[tbl_36];
SELECT nested FROM t WHERE id IN (10797, 10798, 10799);
DELETE FROM bench_t_14 WHERE id = 14;
$dz$ dollar body 10799 ; semicolon inside $dz$
/* block header 10800 */
SELECT `mysql_10801` FROM `tbl_1`;
SELECT * FROM "quoted_10802" WHERE col = E'esc\'10802';
$dz$ dollar body 10803 ; semicolon inside $dz$
-- line 10804: deterministic comment
SELECT * FROM "quoted_10805" WHERE col = E'esc\'10805';
UPDATE bench_t_54 SET payload = 10806 WHERE id = 22;
$dz$ dollar body 10807 ; semicolon inside $dz$
SELECT * FROM "quoted_10808" WHERE col = E'esc\'10808';
SELECT [bracket_10809] FROM [dbo].[tbl_9];
BEGIN; SELECT 10810; COMMIT;
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT [bracket_10812] FROM [dbo].[tbl_12];
UPDATE bench_t_61 SET payload = 10813 WHERE id = 29;
/* block header 10814 */
# hash comment 10815
# hash comment 10816
BEGIN; SELECT 10817; COMMIT;
INSERT INTO bench_t_66 (id, payload) VALUES (10818, 'v10818');
SELECT 10819 AS id, 'row_10819' AS label;
DELETE FROM bench_t_4 WHERE id = 4;
SELECT 10821 AS id, 'row_10821' AS label;
SELECT [bracket_10822] FROM [dbo].[tbl_22];
SELECT `mysql_10823` FROM `tbl_23`;
UPDATE bench_t_8 SET payload = 10824 WHERE id = 8;
BEGIN; SELECT 10825; COMMIT;
BEGIN; SELECT 10826; COMMIT;
# hash comment 10827
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT 10829 AS id, 'row_10829' AS label;
-- line 10830: deterministic comment
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
# hash comment 10832
SELECT [bracket_10833] FROM [dbo].[tbl_33];
UPDATE bench_t_18 SET payload = 10834 WHERE id = 18;
DELETE FROM bench_t_19 WHERE id = 3;
INSERT INTO bench_t_84 (id, payload) VALUES (10836, 'v10836');
/* block header 10837 */
SELECT * FROM "quoted_10838" WHERE col = E'esc\'10838';
INSERT INTO bench_t_87 (id, payload) VALUES (10839, 'v10839');
BEGIN; SELECT 10840; COMMIT;
SELECT 10841 AS id, 'row_10841' AS label;
DELETE FROM bench_t_26 WHERE id = 10;
SELECT * FROM "quoted_10843" WHERE col = E'esc\'10843';
BEGIN; SELECT 10844; COMMIT;
# hash comment 10845
SELECT [bracket_10846] FROM [dbo].[tbl_6];
SELECT [bracket_10847] FROM [dbo].[tbl_7];
# hash comment 10848
# hash comment 10849
WITH cte_10850 AS (SELECT 10850 AS n) SELECT n FROM cte_10850;
DELETE FROM bench_t_3 WHERE id = 3;
INSERT INTO bench_t_100 (id, payload) VALUES (10852, 'v10852');
BEGIN; SELECT 10853; COMMIT;
SELECT nested FROM t WHERE id IN (10854, 10855, 10856);
SELECT [bracket_10855] FROM [dbo].[tbl_15];
SELECT nested FROM t WHERE id IN (10856, 10857, 10858);
SELECT nested FROM t WHERE id IN (10857, 10858, 10859);
SELECT `mysql_10858` FROM `tbl_8`;
SELECT `mysql_10859` FROM `tbl_9`;
-- line 10860: deterministic comment
SELECT 10861 AS id, 'row_10861' AS label;
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
# hash comment 10864
# hash comment 10865
# hash comment 10866
SELECT [bracket_10867] FROM [dbo].[tbl_27];
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
UPDATE bench_t_53 SET payload = 10869 WHERE id = 21;
-- line 10870: deterministic comment
$dz$ dollar body 10871 ; semicolon inside $dz$
SELECT nested FROM t WHERE id IN (10872, 10873, 10874);
# hash comment 10873
SELECT `mysql_10874` FROM `tbl_24`;
/* block header 10875 */
SELECT * FROM "quoted_10876" WHERE col = E'esc\'10876';
SELECT [bracket_10877] FROM [dbo].[tbl_37];
WITH cte_10878 AS (SELECT 10878 AS n) SELECT n FROM cte_10878;
SELECT 10879 AS id, 'row_10879' AS label;
SELECT * FROM "quoted_10880" WHERE col = E'esc\'10880';
INSERT INTO bench_t_1 (id, payload) VALUES (10881, 'v10881');
WITH cte_10882 AS (SELECT 10882 AS n) SELECT n FROM cte_10882;
SELECT 10883 AS id, 'row_10883' AS label;
INSERT INTO bench_t_4 (id, payload) VALUES (10884, 'v10884');
WITH cte_10885 AS (SELECT 10885 AS n) SELECT n FROM cte_10885;
SELECT 10886 AS id, 'row_10886' AS label;
UPDATE bench_t_7 SET payload = 10887 WHERE id = 7;
WITH cte_10888 AS (SELECT 10888 AS n) SELECT n FROM cte_10888;
/* block header 10889 */
$dz$ dollar body 10890 ; semicolon inside $dz$
/* block header 10891 */
INSERT INTO bench_t_12 (id, payload) VALUES (10892, 'v10892');
BEGIN; SELECT 10893; COMMIT;
SELECT * FROM "quoted_10894" WHERE col = E'esc\'10894';
BEGIN; SELECT 10895; COMMIT;
INSERT INTO bench_t_16 (id, payload) VALUES (10896, 'v10896');
WITH cte_10897 AS (SELECT 10897 AS n) SELECT n FROM cte_10897;
$dz$ dollar body 10898 ; semicolon inside $dz$
WITH cte_10899 AS (SELECT 10899 AS n) SELECT n FROM cte_10899;
SELECT [bracket_10900] FROM [dbo].[tbl_20];
INSERT INTO bench_t_21 (id, payload) VALUES (10901, 'O''Brien');
INSERT INTO bench_t_22 (id, payload) VALUES (10902, 'v10902');
SELECT 10903 AS id, 'row_10903' AS label;
/* block header 10904 */
BEGIN; SELECT 10905; COMMIT;
SELECT [bracket_10906] FROM [dbo].[tbl_26];
DELETE FROM bench_t_27 WHERE id = 11;
$dz$ dollar body 10908 ; semicolon inside $dz$
UPDATE bench_t_29 SET payload = 10909 WHERE id = 29;
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT nested FROM t WHERE id IN (10911, 10912, 10913);
SELECT 10912 AS id, 'row_10912' AS label;
SELECT `mysql_10913` FROM `tbl_13`;
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
INSERT INTO bench_t_36 (id, payload) VALUES (10916, 'v10916');
$dz$ dollar body 10917 ; semicolon inside $dz$
-- line 10918: deterministic comment
UPDATE bench_t_39 SET payload = 10919 WHERE id = 7;
UPDATE bench_t_40 SET payload = 10920 WHERE id = 8;
SELECT 10921 AS id, 'row_10921' AS label;
WITH cte_10922 AS (SELECT 10922 AS n) SELECT n FROM cte_10922;
SELECT `mysql_10923` FROM `tbl_23`;
/* block header 10924 */
WITH cte_10925 AS (SELECT 10925 AS n) SELECT n FROM cte_10925;
BEGIN; SELECT 10926; COMMIT;
/* block header 10927 */
UPDATE bench_t_48 SET payload = 10928 WHERE id = 16;
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT `mysql_10930` FROM `tbl_30`;
WITH cte_10931 AS (SELECT 10931 AS n) SELECT n FROM cte_10931;
SELECT nested FROM t WHERE id IN (10932, 10933, 10934);
SELECT [bracket_10933] FROM [dbo].[tbl_13];
WITH cte_10934 AS (SELECT 10934 AS n) SELECT n FROM cte_10934;
# hash comment 10935
INSERT INTO bench_t_56 (id, payload) VALUES (10936, 'v10936');
DELETE FROM bench_t_25 WHERE id = 9;
SELECT `mysql_10938` FROM `tbl_38`;
SELECT `mysql_10939` FROM `tbl_39`;
-- line 10940: deterministic comment
$dz$ dollar body 10941 ; semicolon inside $dz$
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT * FROM "quoted_10943" WHERE col = E'esc\'10943';
INSERT INTO bench_t_64 (id, payload) VALUES (10944, 'v10944');
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT [bracket_10946] FROM [dbo].[tbl_26];
SELECT [bracket_10947] FROM [dbo].[tbl_27];
SELECT * FROM "quoted_10948" WHERE col = E'esc\'10948';
UPDATE bench_t_5 SET payload = 10949 WHERE id = 5;
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT [bracket_10953] FROM [dbo].[tbl_33];
WITH cte_10954 AS (SELECT 10954 AS n) SELECT n FROM cte_10954;
DELETE FROM bench_t_11 WHERE id = 11;
SELECT * FROM "quoted_10956" WHERE col = E'esc\'10956';
SELECT nested FROM t WHERE id IN (10957, 10958, 10959);
-- line 10958: deterministic comment
DELETE FROM bench_t_15 WHERE id = 15;
INSERT INTO bench_t_80 (id, payload) VALUES (10960, 'v10960');
SELECT [bracket_10961] FROM [dbo].[tbl_1];
SELECT * FROM "quoted_10962" WHERE col = E'esc\'10962';
DELETE FROM bench_t_19 WHERE id = 3;
SELECT * FROM "quoted_10964" WHERE col = E'esc\'10964';
INSERT INTO bench_t_85 (id, payload) VALUES (10965, 'v10965');
$dz$ dollar body 10966 ; semicolon inside $dz$
# hash comment 10967
INSERT INTO bench_t_88 (id, payload) VALUES (10968, 'v10968');
SELECT * FROM "quoted_10969" WHERE col = E'esc\'10969';
SELECT nested FROM t WHERE id IN (10970, 10971, 10972);
UPDATE bench_t_27 SET payload = 10971 WHERE id = 27;
# hash comment 10972
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT nested FROM t WHERE id IN (10975, 10976, 10977);
SELECT * FROM "quoted_10976" WHERE col = E'esc\'10976';
DELETE FROM bench_t_1 WHERE id = 1;
# hash comment 10978
/* block header 10979 */
DELETE FROM bench_t_4 WHERE id = 4;
SELECT `mysql_10981` FROM `tbl_31`;
# hash comment 10982
BEGIN; SELECT 10983; COMMIT;
$dz$ dollar body 10984 ; semicolon inside $dz$
SELECT nested FROM t WHERE id IN (10985, 10986, 10987);
SELECT * FROM "quoted_10986" WHERE col = E'esc\'10986';
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT * FROM "quoted_10988" WHERE col = E'esc\'10988';
BEGIN; SELECT 10989; COMMIT;
$dz$ dollar body 10990 ; semicolon inside $dz$
/* block header 10991 */
SELECT `mysql_10992` FROM `tbl_42`;
SELECT [bracket_10993] FROM [dbo].[tbl_33];
WITH cte_10994 AS (SELECT 10994 AS n) SELECT n FROM cte_10994;
BEGIN; SELECT 10995; COMMIT;
SELECT 10996 AS id, 'row_10996' AS label;
SELECT * FROM "quoted_10997" WHERE col = E'esc\'10997';
SELECT [bracket_10998] FROM [dbo].[tbl_38];
BEGIN; SELECT 10999; COMMIT;
/*
 * section 44
 * checksum 8412
 */
SELECT nested FROM t WHERE id IN (11000, 11001, 11002);
/* block header 11005 */
SELECT [bracket_11006] FROM [dbo].[tbl_6];
/* block header 11007 */
# hash comment 11008
# hash comment 11009
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
DELETE FROM bench_t_3 WHERE id = 3;
$dz$ dollar body 11012 ; semicolon inside $dz$
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
BEGIN; SELECT 11014; COMMIT;
SELECT nested FROM t WHERE id IN (11015, 11016, 11017);
UPDATE bench_t_8 SET payload = 11016 WHERE id = 8;
DELETE FROM bench_t_9 WHERE id = 9;
-- line 11018: deterministic comment
INSERT INTO bench_t_11 (id, payload) VALUES (11019, 'v11019');
BEGIN; SELECT 11020; COMMIT;
UPDATE bench_t_13 SET payload = 11021 WHERE id = 13;
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
DELETE FROM bench_t_15 WHERE id = 15;
/* block header 11024 */
UPDATE bench_t_17 SET payload = 11025 WHERE id = 17;
SELECT `mysql_11026` FROM `tbl_26`;
SELECT * FROM "quoted_11027" WHERE col = E'esc\'11027';
DELETE FROM bench_t_20 WHERE id = 4;
WITH cte_11029 AS (SELECT 11029 AS n) SELECT n FROM cte_11029;
SELECT `mysql_11030` FROM `tbl_30`;
SELECT 11031 AS id, 'row_11031' AS label;
SELECT [bracket_11032] FROM [dbo].[tbl_32];
SELECT * FROM "quoted_11033" WHERE col = E'esc\'11033';
/* block header 11034 */
SELECT * FROM "quoted_11035" WHERE col = E'esc\'11035';
SELECT * FROM "quoted_11036" WHERE col = E'esc\'11036';
UPDATE bench_t_29 SET payload = 11037 WHERE id = 29;
SELECT 11038 AS id, 'row_11038' AS label;
SELECT nested FROM t WHERE id IN (11039, 11040, 11041);
$dz$ dollar body 11040 ; semicolon inside $dz$
SELECT nested FROM t WHERE id IN (11041, 11042, 11043);
# hash comment 11042
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
# hash comment 11044
$dz$ dollar body 11045 ; semicolon inside $dz$
SELECT 11046 AS id, 'row_11046' AS label;
/* block header 11047 */
BEGIN; SELECT 11048; COMMIT;
SELECT nested FROM t WHERE id IN (11049, 11050, 11051);
BEGIN; SELECT 11050; COMMIT;
UPDATE bench_t_43 SET payload = 11051 WHERE id = 11;
/* block header 11052 */
SELECT * FROM "quoted_11053" WHERE col = E'esc\'11053';
$dz$ dollar body 11054 ; semicolon inside $dz$
SELECT * FROM "quoted_11055" WHERE col = E'esc\'11055';
SELECT * FROM "quoted_11056" WHERE col = E'esc\'11056';
# hash comment 11057
SELECT [bracket_11058] FROM [dbo].[tbl_18];
SELECT [bracket_11059] FROM [dbo].[tbl_19];
SELECT [bracket_11060] FROM [dbo].[tbl_20];
SELECT 11061 AS id, 'row_11061' AS label;
SELECT [bracket_11062] FROM [dbo].[tbl_22];
SELECT 11063 AS id, 'row_11063' AS label;
SELECT * FROM "quoted_11064" WHERE col = E'esc\'11064';
SELECT nested FROM t WHERE id IN (11065, 11066, 11067);
SELECT 11066 AS id, 'row_11066' AS label;
WITH cte_11067 AS (SELECT 11067 AS n) SELECT n FROM cte_11067;
SELECT [bracket_11068] FROM [dbo].[tbl_28];
BEGIN; SELECT 11069; COMMIT;
/* block header 11070 */
# hash comment 11071
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT nested FROM t WHERE id IN (11073, 11074, 11075);
UPDATE bench_t_2 SET payload = 11074 WHERE id = 2;
SELECT * FROM "quoted_11075" WHERE col = E'esc\'11075';
DELETE FROM bench_t_4 WHERE id = 4;
# hash comment 11077
SELECT 11078 AS id, 'row_11078' AS label;
WITH cte_11079 AS (SELECT 11079 AS n) SELECT n FROM cte_11079;
INSERT INTO bench_t_72 (id, payload) VALUES (11080, 'v11080');
UPDATE bench_t_9 SET payload = 11081 WHERE id = 9;
WITH cte_11082 AS (SELECT 11082 AS n) SELECT n FROM cte_11082;
INSERT INTO bench_t_75 (id, payload) VALUES (11083, 'v11083');
DELETE FROM bench_t_12 WHERE id = 12;
WITH cte_11085 AS (SELECT 11085 AS n) SELECT n FROM cte_11085;
# hash comment 11086
SELECT * FROM "quoted_11087" WHERE col = E'esc\'11087';
SELECT `mysql_11088` FROM `tbl_38`;
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT 11090 AS id, 'row_11090' AS label;
SELECT `mysql_11091` FROM `tbl_41`;
WITH cte_11092 AS (SELECT 11092 AS n) SELECT n FROM cte_11092;
WITH cte_11093 AS (SELECT 11093 AS n) SELECT n FROM cte_11093;
SELECT * FROM "quoted_11094" WHERE col = E'esc\'11094';
BEGIN; SELECT 11095; COMMIT;
BEGIN; SELECT 11096; COMMIT;
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
UPDATE bench_t_26 SET payload = 11098 WHERE id = 26;
/* block header 11099 */
SELECT nested FROM t WHERE id IN (11100, 11101, 11102);
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT nested FROM t WHERE id IN (11102, 11103, 11104);
SELECT * FROM "quoted_11103" WHERE col = E'esc\'11103';
SELECT [bracket_11104] FROM [dbo].[tbl_24];
SELECT [bracket_11105] FROM [dbo].[tbl_25];
# hash comment 11106
/* block header 11107 */
SELECT `mysql_11108` FROM `tbl_8`;
/* block header 11109 */
SELECT * FROM "quoted_11110" WHERE col = E'esc\'11110';
DELETE FROM bench_t_7 WHERE id = 7;
# hash comment 11112
SELECT 11113 AS id, 'row_11113' AS label;
BEGIN; SELECT 11114; COMMIT;
-- line 11115: deterministic comment
-- line 11116: deterministic comment
$dz$ dollar body 11117 ; semicolon inside $dz$
SELECT nested FROM t WHERE id IN (11118, 11119, 11120);
/* block header 11119 */
# hash comment 11120
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
/* block header 11122 */
SELECT nested FROM t WHERE id IN (11123, 11124, 11125);
DELETE FROM bench_t_20 WHERE id = 4;
SELECT `mysql_11125` FROM `tbl_25`;
SELECT nested FROM t WHERE id IN (11126, 11127, 11128);
-- line 11127: deterministic comment
SELECT * FROM "quoted_11128" WHERE col = E'esc\'11128';
SELECT * FROM "quoted_11129" WHERE col = E'esc\'11129';
UPDATE bench_t_58 SET payload = 11130 WHERE id = 26;
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
UPDATE bench_t_60 SET payload = 11132 WHERE id = 28;
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
$dz$ dollar body 11134 ; semicolon inside $dz$
INSERT INTO bench_t_127 (id, payload) VALUES (11135, 'v11135');
INSERT INTO bench_t_0 (id, payload) VALUES (11136, 'v11136');
SELECT nested FROM t WHERE id IN (11137, 11138, 11139);
SELECT `mysql_11138` FROM `tbl_38`;
BEGIN; SELECT 11139; COMMIT;
$dz$ dollar body 11140 ; semicolon inside $dz$
SELECT * FROM "quoted_11141" WHERE col = E'esc\'11141';
BEGIN; SELECT 11142; COMMIT;
UPDATE bench_t_7 SET payload = 11143 WHERE id = 7;
BEGIN; SELECT 11144; COMMIT;
-- line 11145: deterministic comment
SELECT `mysql_11146` FROM `tbl_46`;
INSERT INTO bench_t_11 (id, payload) VALUES (11147, 'v11147');
BEGIN; SELECT 11148; COMMIT;
SELECT * FROM "quoted_11149" WHERE col = E'esc\'11149';
SELECT * FROM "quoted_11150" WHERE col = E'esc\'11150';
/* block header 11151 */
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
DELETE FROM bench_t_17 WHERE id = 1;
INSERT INTO bench_t_18 (id, payload) VALUES (11154, 'O''Brien');
SELECT [bracket_11155] FROM [dbo].[tbl_35];
INSERT INTO bench_t_20 (id, payload) VALUES (11156, 'v11156');
# hash comment 11157
$dz$ dollar body 11158 ; semicolon inside $dz$
# hash comment 11159
UPDATE bench_t_24 SET payload = 11160 WHERE id = 24;
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT * FROM "quoted_11162" WHERE col = E'esc\'11162';
/* block header 11163 */
/* block header 11164 */
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT nested FROM t WHERE id IN (11166, 11167, 11168);
DELETE FROM bench_t_31 WHERE id = 15;
SELECT [bracket_11168] FROM [dbo].[tbl_8];
SELECT `mysql_11169` FROM `tbl_19`;
WITH cte_11170 AS (SELECT 11170 AS n) SELECT n FROM cte_11170;
BEGIN; SELECT 11171; COMMIT;
/* block header 11172 */
WITH cte_11173 AS (SELECT 11173 AS n) SELECT n FROM cte_11173;
UPDATE bench_t_38 SET payload = 11174 WHERE id = 6;
-- line 11175: deterministic comment
-- line 11176: deterministic comment
SELECT * FROM "quoted_11177" WHERE col = E'esc\'11177';
# hash comment 11178
# hash comment 11179
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
/* block header 11181 */
DELETE FROM bench_t_14 WHERE id = 14;
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
/* block header 11184 */
-- line 11185: deterministic comment
/* block header 11186 */
/* block header 11187 */
BEGIN; SELECT 11188; COMMIT;
-- line 11189: deterministic comment
WITH cte_11190 AS (SELECT 11190 AS n) SELECT n FROM cte_11190;
UPDATE bench_t_55 SET payload = 11191 WHERE id = 23;
SELECT * FROM "quoted_11192" WHERE col = E'esc\'11192';
SELECT 11193 AS id, 'row_11193' AS label;
-- line 11194: deterministic comment
DELETE FROM bench_t_27 WHERE id = 11;
SELECT [bracket_11196] FROM [dbo].[tbl_36];
DELETE FROM bench_t_29 WHERE id = 13;
DELETE FROM bench_t_30 WHERE id = 14;
SELECT nested FROM t WHERE id IN (11199, 11200, 11201);
UPDATE bench_t_0 SET payload = 11200 WHERE id = 0;
INSERT INTO bench_t_65 (id, payload) VALUES (11201, 'v11201');
/* block header 11202 */
SELECT 11203 AS id, 'row_11203' AS label;
DELETE FROM bench_t_4 WHERE id = 4;
DELETE FROM bench_t_5 WHERE id = 5;
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT * FROM "quoted_11207" WHERE col = E'esc\'11207';
$dz$ dollar body 11208 ; semicolon inside $dz$
/* block header 11209 */
SELECT * FROM "quoted_11210" WHERE col = E'esc\'11210';
WITH cte_11211 AS (SELECT 11211 AS n) SELECT n FROM cte_11211;
UPDATE bench_t_12 SET payload = 11212 WHERE id = 12;
DELETE FROM bench_t_13 WHERE id = 13;
SELECT [bracket_11214] FROM [dbo].[tbl_14];
UPDATE bench_t_15 SET payload = 11215 WHERE id = 15;
-- line 11216: deterministic comment
UPDATE bench_t_17 SET payload = 11217 WHERE id = 17;
/* block header 11218 */
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT 11220 AS id, 'row_11220' AS label;
# hash comment 11221
SELECT nested FROM t WHERE id IN (11222, 11223, 11224);
WITH cte_11223 AS (SELECT 11223 AS n) SELECT n FROM cte_11223;
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
/* block header 11225 */
/* block header 11226 */
INSERT INTO bench_t_91 (id, payload) VALUES (11227, 'v11227');
# hash comment 11228
SELECT nested FROM t WHERE id IN (11229, 11230, 11231);
# hash comment 11230
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
# hash comment 11232
$dz$ dollar body 11233 ; semicolon inside $dz$
/* block header 11234 */
WITH cte_11235 AS (SELECT 11235 AS n) SELECT n FROM cte_11235;
# hash comment 11236
# hash comment 11237
/* block header 11238 */
SELECT `mysql_11239` FROM `tbl_39`;
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT * FROM "quoted_11241" WHERE col = E'esc\'11241';
$dz$ dollar body 11242 ; semicolon inside $dz$
-- line 11243: deterministic comment
-- line 11244: deterministic comment
SELECT `mysql_11245` FROM `tbl_45`;
/* block header 11246 */
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
WITH cte_11248 AS (SELECT 11248 AS n) SELECT n FROM cte_11248;
SELECT * FROM "quoted_11249" WHERE col = E'esc\'11249';
/*
 * section 45
 * checksum ef22
 */
BEGIN; SELECT 11250; COMMIT;
SELECT * FROM "quoted_11255" WHERE col = E'esc\'11255';
-- line 11256: deterministic comment
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT `mysql_11258` FROM `tbl_8`;
$dz$ dollar body 11259 ; semicolon inside $dz$
SELECT 11260 AS id, 'row_11260' AS label;
-- line 11261: deterministic comment
-- line 11262: deterministic comment
-- line 11263: deterministic comment
INSERT INTO bench_t_0 (id, payload) VALUES (11264, 'O''Brien');
/* block header 11265 */
$dz$ dollar body 11266 ; semicolon inside $dz$
UPDATE bench_t_3 SET payload = 11267 WHERE id = 3;
SELECT * FROM "quoted_11268" WHERE col = E'esc\'11268';
INSERT INTO bench_t_5 (id, payload) VALUES (11269, 'v11269');
BEGIN; SELECT 11270; COMMIT;
SELECT nested FROM t WHERE id IN (11271, 11272, 11273);
SELECT nested FROM t WHERE id IN (11272, 11273, 11274);
BEGIN; SELECT 11273; COMMIT;
# hash comment 11274
SELECT `mysql_11275` FROM `tbl_25`;
# hash comment 11276
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
$dz$ dollar body 11278 ; semicolon inside $dz$
-- line 11279: deterministic comment
SELECT `mysql_11280` FROM `tbl_30`;
SELECT * FROM "quoted_11281" WHERE col = E'esc\'11281';
-- line 11282: deterministic comment
$dz$ dollar body 11283 ; semicolon inside $dz$
# hash comment 11284
SELECT `mysql_11285` FROM `tbl_35`;
WITH cte_11286 AS (SELECT 11286 AS n) SELECT n FROM cte_11286;
SELECT `mysql_11287` FROM `tbl_37`;
WITH cte_11288 AS (SELECT 11288 AS n) SELECT n FROM cte_11288;
DELETE FROM bench_t_25 WHERE id = 9;
WITH cte_11290 AS (SELECT 11290 AS n) SELECT n FROM cte_11290;
$dz$ dollar body 11291 ; semicolon inside $dz$
SELECT [bracket_11292] FROM [dbo].[tbl_12];
DELETE FROM bench_t_29 WHERE id = 13;
$dz$ dollar body 11294 ; semicolon inside $dz$
$dz$ dollar body 11295 ; semicolon inside $dz$
-- line 11296: deterministic comment
SELECT * FROM "quoted_11297" WHERE col = E'esc\'11297';
SELECT 11298 AS id, 'row_11298' AS label;
WITH cte_11299 AS (SELECT 11299 AS n) SELECT n FROM cte_11299;
SELECT 11300 AS id, 'row_11300' AS label;
UPDATE bench_t_37 SET payload = 11301 WHERE id = 5;
INSERT INTO bench_t_38 (id, payload) VALUES (11302, 'v11302');
BEGIN; SELECT 11303; COMMIT;
UPDATE bench_t_40 SET payload = 11304 WHERE id = 8;
SELECT `mysql_11305` FROM `tbl_5`;
INSERT INTO bench_t_42 (id, payload) VALUES (11306, 'v11306');
SELECT `mysql_11307` FROM `tbl_7`;
SELECT `mysql_11308` FROM `tbl_8`;
# hash comment 11309
BEGIN; SELECT 11310; COMMIT;
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
UPDATE bench_t_48 SET payload = 11312 WHERE id = 16;
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
# hash comment 11314
SELECT nested FROM t WHERE id IN (11315, 11316, 11317);
# hash comment 11316
UPDATE bench_t_53 SET payload = 11317 WHERE id = 21;
UPDATE bench_t_54 SET payload = 11318 WHERE id = 22;
SELECT 11319 AS id, 'row_11319' AS label;
-- line 11320: deterministic comment
SELECT * FROM "quoted_11321" WHERE col = E'esc\'11321';
SELECT * FROM "quoted_11322" WHERE col = E'esc\'11322';
$dz$ dollar body 11323 ; semicolon inside $dz$
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
/* block header 11325 */
SELECT [bracket_11326] FROM [dbo].[tbl_6];
WITH cte_11327 AS (SELECT 11327 AS n) SELECT n FROM cte_11327;
WITH cte_11328 AS (SELECT 11328 AS n) SELECT n FROM cte_11328;
SELECT nested FROM t WHERE id IN (11329, 11330, 11331);
SELECT * FROM "quoted_11330" WHERE col = E'esc\'11330';
WITH cte_11331 AS (SELECT 11331 AS n) SELECT n FROM cte_11331;
UPDATE bench_t_4 SET payload = 11332 WHERE id = 4;
SELECT nested FROM t WHERE id IN (11333, 11334, 11335);
-- line 11334: deterministic comment
SELECT `mysql_11335` FROM `tbl_35`;
UPDATE bench_t_8 SET payload = 11336 WHERE id = 8;
SELECT * FROM "quoted_11337" WHERE col = E'esc\'11337';
# hash comment 11338
UPDATE bench_t_11 SET payload = 11339 WHERE id = 11;
/* block header 11340 */
BEGIN; SELECT 11341; COMMIT;
-- line 11342: deterministic comment
INSERT INTO bench_t_79 (id, payload) VALUES (11343, 'v11343');
INSERT INTO bench_t_80 (id, payload) VALUES (11344, 'v11344');
$dz$ dollar body 11345 ; semicolon inside $dz$
$dz$ dollar body 11346 ; semicolon inside $dz$
DELETE FROM bench_t_19 WHERE id = 3;
WITH cte_11348 AS (SELECT 11348 AS n) SELECT n FROM cte_11348;
-- line 11349: deterministic comment
/* block header 11350 */
BEGIN; SELECT 11351; COMMIT;
SELECT [bracket_11352] FROM [dbo].[tbl_32];
UPDATE bench_t_25 SET payload = 11353 WHERE id = 25;
UPDATE bench_t_26 SET payload = 11354 WHERE id = 26;
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
/* block header 11356 */
SELECT 11357 AS id, 'row_11357' AS label;
# hash comment 11358
$dz$ dollar body 11359 ; semicolon inside $dz$
UPDATE bench_t_32 SET payload = 11360 WHERE id = 0;
# hash comment 11361
SELECT nested FROM t WHERE id IN (11362, 11363, 11364);
SELECT nested FROM t WHERE id IN (11363, 11364, 11365);
SELECT `mysql_11364` FROM `tbl_14`;
SELECT [bracket_11365] FROM [dbo].[tbl_5];
WITH cte_11366 AS (SELECT 11366 AS n) SELECT n FROM cte_11366;
-- line 11367: deterministic comment
/* block header 11368 */
INSERT INTO bench_t_105 (id, payload) VALUES (11369, 'v11369');
SELECT 11370 AS id, 'row_11370' AS label;
SELECT nested FROM t WHERE id IN (11371, 11372, 11373);
WITH cte_11372 AS (SELECT 11372 AS n) SELECT n FROM cte_11372;
SELECT 11373 AS id, 'row_11373' AS label;
SELECT `mysql_11374` FROM `tbl_24`;
BEGIN; SELECT 11375; COMMIT;
SELECT `mysql_11376` FROM `tbl_26`;
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
# hash comment 11378
SELECT [bracket_11379] FROM [dbo].[tbl_19];
SELECT [bracket_11380] FROM [dbo].[tbl_20];
BEGIN; SELECT 11381; COMMIT;
WITH cte_11382 AS (SELECT 11382 AS n) SELECT n FROM cte_11382;
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
BEGIN; SELECT 11384; COMMIT;
-- line 11385: deterministic comment
INSERT INTO bench_t_122 (id, payload) VALUES (11386, 'v11386');
INSERT INTO bench_t_123 (id, payload) VALUES (11387, 'v11387');
# hash comment 11388
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
-- line 11390: deterministic comment
$dz$ dollar body 11391 ; semicolon inside $dz$
SELECT [bracket_11392] FROM [dbo].[tbl_32];
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT `mysql_11394` FROM `tbl_44`;
SELECT * FROM "quoted_11395" WHERE col = E'esc\'11395';
-- line 11396: deterministic comment
$dz$ dollar body 11397 ; semicolon inside $dz$
BEGIN; SELECT 11398; COMMIT;
BEGIN; SELECT 11399; COMMIT;
DELETE FROM bench_t_8 WHERE id = 8;
-- line 11401: deterministic comment
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
WITH cte_11403 AS (SELECT 11403 AS n) SELECT n FROM cte_11403;
$dz$ dollar body 11404 ; semicolon inside $dz$
SELECT nested FROM t WHERE id IN (11405, 11406, 11407);
DELETE FROM bench_t_14 WHERE id = 14;
-- line 11407: deterministic comment
-- line 11408: deterministic comment
SELECT nested FROM t WHERE id IN (11409, 11410, 11411);
$dz$ dollar body 11410 ; semicolon inside $dz$
SELECT nested FROM t WHERE id IN (11411, 11412, 11413);
WITH cte_11412 AS (SELECT 11412 AS n) SELECT n FROM cte_11412;
WITH cte_11413 AS (SELECT 11413 AS n) SELECT n FROM cte_11413;
/* block header 11414 */
INSERT INTO bench_t_23 (id, payload) VALUES (11415, 'v11415');
-- line 11416: deterministic comment
UPDATE bench_t_25 SET payload = 11417 WHERE id = 25;
UPDATE bench_t_26 SET payload = 11418 WHERE id = 26;
SELECT nested FROM t WHERE id IN (11419, 11420, 11421);
/* block header 11420 */
$dz$ dollar body 11421 ; semicolon inside $dz$
SELECT * FROM "quoted_11422" WHERE col = E'esc\'11422';
/* block header 11423 */
-- line 11424: deterministic comment
DELETE FROM bench_t_1 WHERE id = 1;
SELECT 11426 AS id, 'row_11426' AS label;
SELECT * FROM "quoted_11427" WHERE col = E'esc\'11427';
DELETE FROM bench_t_4 WHERE id = 4;
SELECT [bracket_11429] FROM [dbo].[tbl_29];
DELETE FROM bench_t_6 WHERE id = 6;
SELECT nested FROM t WHERE id IN (11431, 11432, 11433);
$dz$ dollar body 11432 ; semicolon inside $dz$
SELECT `mysql_11433` FROM `tbl_33`;
-- line 11434: deterministic comment
SELECT 11435 AS id, 'row_11435' AS label;
BEGIN; SELECT 11436; COMMIT;
/* block header 11437 */
SELECT `mysql_11438` FROM `tbl_38`;
BEGIN; SELECT 11439; COMMIT;
UPDATE bench_t_48 SET payload = 11440 WHERE id = 16;
SELECT 11441 AS id, 'row_11441' AS label;
-- line 11442: deterministic comment
$dz$ dollar body 11443 ; semicolon inside $dz$
/* block header 11444 */
INSERT INTO bench_t_53 (id, payload) VALUES (11445, 'v11445');
SELECT 11446 AS id, 'row_11446' AS label;
/* block header 11447 */
# hash comment 11448
UPDATE bench_t_57 SET payload = 11449 WHERE id = 25;
/* block header 11450 */
SELECT * FROM "quoted_11451" WHERE col = E'esc\'11451';
/* block header 11452 */
/* block header 11453 */
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
UPDATE bench_t_63 SET payload = 11455 WHERE id = 31;
# hash comment 11456
DELETE FROM bench_t_1 WHERE id = 1;
DELETE FROM bench_t_2 WHERE id = 2;
SELECT [bracket_11459] FROM [dbo].[tbl_19];
# hash comment 11460
INSERT INTO bench_t_69 (id, payload) VALUES (11461, 'v11461');
WITH cte_11462 AS (SELECT 11462 AS n) SELECT n FROM cte_11462;
UPDATE bench_t_7 SET payload = 11463 WHERE id = 7;
SELECT nested FROM t WHERE id IN (11464, 11465, 11466);
-- line 11465: deterministic comment
UPDATE bench_t_10 SET payload = 11466 WHERE id = 10;
SELECT nested FROM t WHERE id IN (11467, 11468, 11469);
SELECT 11468 AS id, 'row_11468' AS label;
SELECT `mysql_11469` FROM `tbl_19`;
INSERT INTO bench_t_78 (id, payload) VALUES (11470, 'v11470');
BEGIN; SELECT 11471; COMMIT;
WITH cte_11472 AS (SELECT 11472 AS n) SELECT n FROM cte_11472;
UPDATE bench_t_17 SET payload = 11473 WHERE id = 17;
SELECT nested FROM t WHERE id IN (11474, 11475, 11476);
SELECT [bracket_11475] FROM [dbo].[tbl_35];
SELECT 11476 AS id, 'row_11476' AS label;
SELECT nested FROM t WHERE id IN (11477, 11478, 11479);
BEGIN; SELECT 11478; COMMIT;
DELETE FROM bench_t_23 WHERE id = 7;
WITH cte_11480 AS (SELECT 11480 AS n) SELECT n FROM cte_11480;
SELECT 11481 AS id, 'row_11481' AS label;
$dz$ dollar body 11482 ; semicolon inside $dz$
INSERT INTO bench_t_91 (id, payload) VALUES (11483, 'v11483');
/* block header 11484 */
SELECT [bracket_11485] FROM [dbo].[tbl_5];
DELETE FROM bench_t_30 WHERE id = 14;
WITH cte_11487 AS (SELECT 11487 AS n) SELECT n FROM cte_11487;
SELECT nested FROM t WHERE id IN (11488, 11489, 11490);
$dz$ dollar body 11489 ; semicolon inside $dz$
# hash comment 11490
DELETE FROM bench_t_3 WHERE id = 3;
WITH cte_11492 AS (SELECT 11492 AS n) SELECT n FROM cte_11492;
-- line 11493: deterministic comment
# hash comment 11494
WITH cte_11495 AS (SELECT 11495 AS n) SELECT n FROM cte_11495;
UPDATE bench_t_40 SET payload = 11496 WHERE id = 8;
$dz$ dollar body 11497 ; semicolon inside $dz$
DELETE FROM bench_t_10 WHERE id = 10;
/* block header 11499 */
/*
 * section 46
 * checksum 4942
 */
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
DELETE FROM bench_t_17 WHERE id = 1;
SELECT `mysql_11506` FROM `tbl_6`;
/* block header 11507 */
# hash comment 11508
SELECT nested FROM t WHERE id IN (11509, 11510, 11511);
DELETE FROM bench_t_22 WHERE id = 6;
SELECT * FROM "quoted_11511" WHERE col = E'esc\'11511';
SELECT `mysql_11512` FROM `tbl_12`;
DELETE FROM bench_t_25 WHERE id = 9;
SELECT [bracket_11514] FROM [dbo].[tbl_34];
-- line 11515: deterministic comment
/* block header 11516 */
BEGIN; SELECT 11517; COMMIT;
SELECT [bracket_11518] FROM [dbo].[tbl_38];
BEGIN; SELECT 11519; COMMIT;
SELECT 11520 AS id, 'row_11520' AS label;
INSERT INTO bench_t_1 (id, payload) VALUES (11521, 'v11521');
SELECT * FROM "quoted_11522" WHERE col = E'esc\'11522';
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
INSERT INTO bench_t_4 (id, payload) VALUES (11524, 'v11524');
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
UPDATE bench_t_6 SET payload = 11526 WHERE id = 6;
UPDATE bench_t_7 SET payload = 11527 WHERE id = 7;
$dz$ dollar body 11528 ; semicolon inside $dz$
SELECT [bracket_11529] FROM [dbo].[tbl_9];
$dz$ dollar body 11530 ; semicolon inside $dz$
/* block header 11531 */
BEGIN; SELECT 11532; COMMIT;
DELETE FROM bench_t_13 WHERE id = 13;
INSERT INTO bench_t_14 (id, payload) VALUES (11534, 'v11534');
DELETE FROM bench_t_15 WHERE id = 15;
BEGIN; SELECT 11536; COMMIT;
DELETE FROM bench_t_17 WHERE id = 1;
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
DELETE FROM bench_t_19 WHERE id = 3;
SELECT 11540 AS id, 'row_11540' AS label;
SELECT * FROM "quoted_11541" WHERE col = E'esc\'11541';
SELECT [bracket_11542] FROM [dbo].[tbl_22];
/* block header 11543 */
SELECT 11544 AS id, 'row_11544' AS label;
-- line 11545: deterministic comment
SELECT [bracket_11546] FROM [dbo].[tbl_26];
SELECT [bracket_11547] FROM [dbo].[tbl_27];
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT [bracket_11549] FROM [dbo].[tbl_29];
INSERT INTO bench_t_30 (id, payload) VALUES (11550, 'O''Brien');
SELECT nested FROM t WHERE id IN (11551, 11552, 11553);
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
# hash comment 11553
SELECT 11554 AS id, 'row_11554' AS label;
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT * FROM "quoted_11556" WHERE col = E'esc\'11556';
SELECT * FROM "quoted_11557" WHERE col = E'esc\'11557';
UPDATE bench_t_38 SET payload = 11558 WHERE id = 6;
# hash comment 11559
/* block header 11560 */
BEGIN; SELECT 11561; COMMIT;
$dz$ dollar body 11562 ; semicolon inside $dz$
SELECT nested FROM t WHERE id IN (11563, 11564, 11565);
UPDATE bench_t_44 SET payload = 11564 WHERE id = 12;
WITH cte_11565 AS (SELECT 11565 AS n) SELECT n FROM cte_11565;
$dz$ dollar body 11566 ; semicolon inside $dz$
WITH cte_11567 AS (SELECT 11567 AS n) SELECT n FROM cte_11567;
# hash comment 11568
SELECT * FROM "quoted_11569" WHERE col = E'esc\'11569';
DELETE FROM bench_t_18 WHERE id = 2;
WITH cte_11571 AS (SELECT 11571 AS n) SELECT n FROM cte_11571;
SELECT `mysql_11572` FROM `tbl_22`;
-- line 11573: deterministic comment
UPDATE bench_t_54 SET payload = 11574 WHERE id = 22;
INSERT INTO bench_t_55 (id, payload) VALUES (11575, 'v11575');
DELETE FROM bench_t_24 WHERE id = 8;
WITH cte_11577 AS (SELECT 11577 AS n) SELECT n FROM cte_11577;
UPDATE bench_t_58 SET payload = 11578 WHERE id = 26;
/* block header 11579 */
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
DELETE FROM bench_t_29 WHERE id = 13;
SELECT 11582 AS id, 'row_11582' AS label;
WITH cte_11583 AS (SELECT 11583 AS n) SELECT n FROM cte_11583;
SELECT `mysql_11584` FROM `tbl_34`;
UPDATE bench_t_1 SET payload = 11585 WHERE id = 1;
SELECT nested FROM t WHERE id IN (11586, 11587, 11588);
# hash comment 11587
SELECT `mysql_11588` FROM `tbl_38`;
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
INSERT INTO bench_t_70 (id, payload) VALUES (11590, 'v11590');
UPDATE bench_t_7 SET payload = 11591 WHERE id = 7;
SELECT 11592 AS id, 'row_11592' AS label;
BEGIN; SELECT 11593; COMMIT;
DELETE FROM bench_t_10 WHERE id = 10;
SELECT * FROM "quoted_11595" WHERE col = E'esc\'11595';
SELECT [bracket_11596] FROM [dbo].[tbl_36];
SELECT 11597 AS id, 'row_11597' AS label;
UPDATE bench_t_14 SET payload = 11598 WHERE id = 14;
# hash comment 11599
SELECT nested FROM t WHERE id IN (11600, 11601, 11602);
-- line 11601: deterministic comment
SELECT `mysql_11602` FROM `tbl_2`;
UPDATE bench_t_19 SET payload = 11603 WHERE id = 19;
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
$dz$ dollar body 11605 ; semicolon inside $dz$
$dz$ dollar body 11606 ; semicolon inside $dz$
SELECT [bracket_11607] FROM [dbo].[tbl_7];
/* block header 11608 */
/* block header 11609 */
SELECT * FROM "quoted_11610" WHERE col = E'esc\'11610';
SELECT nested FROM t WHERE id IN (11611, 11612, 11613);
$dz$ dollar body 11612 ; semicolon inside $dz$
SELECT `mysql_11613` FROM `tbl_13`;
SELECT * FROM "quoted_11614" WHERE col = E'esc\'11614';
SELECT `mysql_11615` FROM `tbl_15`;
INSERT INTO bench_t_96 (id, payload) VALUES (11616, 'O''Brien');
INSERT INTO bench_t_97 (id, payload) VALUES (11617, 'v11617');
SELECT * FROM "quoted_11618" WHERE col = E'esc\'11618';
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
# hash comment 11620
SELECT [bracket_11621] FROM [dbo].[tbl_21];
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
-- line 11623: deterministic comment
UPDATE bench_t_40 SET payload = 11624 WHERE id = 8;
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT * FROM "quoted_11626" WHERE col = E'esc\'11626';
SELECT `mysql_11627` FROM `tbl_27`;
UPDATE bench_t_44 SET payload = 11628 WHERE id = 12;
WITH cte_11629 AS (SELECT 11629 AS n) SELECT n FROM cte_11629;
INSERT INTO bench_t_110 (id, payload) VALUES (11630, 'v11630');
SELECT [bracket_11631] FROM [dbo].[tbl_31];
DELETE FROM bench_t_16 WHERE id = 0;
SELECT `mysql_11633` FROM `tbl_33`;
WITH cte_11634 AS (SELECT 11634 AS n) SELECT n FROM cte_11634;
BEGIN; SELECT 11635; COMMIT;
WITH cte_11636 AS (SELECT 11636 AS n) SELECT n FROM cte_11636;
SELECT * FROM "quoted_11637" WHERE col = E'esc\'11637';
/* block header 11638 */
BEGIN; SELECT 11639; COMMIT;
# hash comment 11640
SELECT * FROM "quoted_11641" WHERE col = E'esc\'11641';
BEGIN; SELECT 11642; COMMIT;
-- line 11643: deterministic comment
SELECT [bracket_11644] FROM [dbo].[tbl_4];
SELECT [bracket_11645] FROM [dbo].[tbl_5];
# hash comment 11646
BEGIN; SELECT 11647; COMMIT;
$dz$ dollar body 11648 ; semicolon inside $dz$
BEGIN; SELECT 11649; COMMIT;
SELECT nested FROM t WHERE id IN (11650, 11651, 11652);
# hash comment 11651
INSERT INTO bench_t_4 (id, payload) VALUES (11652, 'v11652');
SELECT [bracket_11653] FROM [dbo].[tbl_13];
UPDATE bench_t_6 SET payload = 11654 WHERE id = 6;
SELECT nested FROM t WHERE id IN (11655, 11656, 11657);
-- line 11656: deterministic comment
SELECT [bracket_11657] FROM [dbo].[tbl_17];
DELETE FROM bench_t_10 WHERE id = 10;
DELETE FROM bench_t_11 WHERE id = 11;
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT nested FROM t WHERE id IN (11661, 11662, 11663);
SELECT nested FROM t WHERE id IN (11662, 11663, 11664);
SELECT nested FROM t WHERE id IN (11663, 11664, 11665);
SELECT 11664 AS id, 'row_11664' AS label;
SELECT [bracket_11665] FROM [dbo].[tbl_25];
INSERT INTO bench_t_18 (id, payload) VALUES (11666, 'v11666');
SELECT 11667 AS id, 'row_11667' AS label;
/* block header 11668 */
-- line 11669: deterministic comment
SELECT [bracket_11670] FROM [dbo].[tbl_30];
INSERT INTO bench_t_23 (id, payload) VALUES (11671, 'O''Brien');
-- line 11672: deterministic comment
SELECT * FROM "quoted_11673" WHERE col = E'esc\'11673';
WITH cte_11674 AS (SELECT 11674 AS n) SELECT n FROM cte_11674;
-- line 11675: deterministic comment
UPDATE bench_t_28 SET payload = 11676 WHERE id = 28;
INSERT INTO bench_t_29 (id, payload) VALUES (11677, 'v11677');
SELECT nested FROM t WHERE id IN (11678, 11679, 11680);
/* block header 11679 */
UPDATE bench_t_32 SET payload = 11680 WHERE id = 0;
UPDATE bench_t_33 SET payload = 11681 WHERE id = 1;
SELECT nested FROM t WHERE id IN (11682, 11683, 11684);
INSERT INTO bench_t_35 (id, payload) VALUES (11683, 'v11683');
UPDATE bench_t_36 SET payload = 11684 WHERE id = 4;
SELECT 11685 AS id, 'row_11685' AS label;
# hash comment 11686
SELECT 11687 AS id, 'row_11687' AS label;
-- line 11688: deterministic comment
$dz$ dollar body 11689 ; semicolon inside $dz$
# hash comment 11690
DELETE FROM bench_t_11 WHERE id = 11;
# hash comment 11692
SELECT `mysql_11693` FROM `tbl_43`;
SELECT nested FROM t WHERE id IN (11694, 11695, 11696);
SELECT * FROM "quoted_11695" WHERE col = E'esc\'11695';
# hash comment 11696
$dz$ dollar body 11697 ; semicolon inside $dz$
# hash comment 11698
-- line 11699: deterministic comment
INSERT INTO bench_t_52 (id, payload) VALUES (11700, 'v11700');
INSERT INTO bench_t_53 (id, payload) VALUES (11701, 'v11701');
-- line 11702: deterministic comment
SELECT 11703 AS id, 'row_11703' AS label;
BEGIN; SELECT 11704; COMMIT;
UPDATE bench_t_57 SET payload = 11705 WHERE id = 25;
SELECT `mysql_11706` FROM `tbl_6`;
WITH cte_11707 AS (SELECT 11707 AS n) SELECT n FROM cte_11707;
INSERT INTO bench_t_60 (id, payload) VALUES (11708, 'v11708');
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
WITH cte_11711 AS (SELECT 11711 AS n) SELECT n FROM cte_11711;
# hash comment 11712
$dz$ dollar body 11713 ; semicolon inside $dz$
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
WITH cte_11715 AS (SELECT 11715 AS n) SELECT n FROM cte_11715;
BEGIN; SELECT 11716; COMMIT;
$dz$ dollar body 11717 ; semicolon inside $dz$
DELETE FROM bench_t_6 WHERE id = 6;
SELECT * FROM "quoted_11719" WHERE col = E'esc\'11719';
INSERT INTO bench_t_72 (id, payload) VALUES (11720, 'v11720');
DELETE FROM bench_t_9 WHERE id = 9;
SELECT `mysql_11722` FROM `tbl_22`;
-- line 11723: deterministic comment
SELECT nested FROM t WHERE id IN (11724, 11725, 11726);
SELECT `mysql_11725` FROM `tbl_25`;
/* block header 11726 */
UPDATE bench_t_15 SET payload = 11727 WHERE id = 15;
SELECT * FROM "quoted_11728" WHERE col = E'esc\'11728';
SELECT [bracket_11729] FROM [dbo].[tbl_9];
$dz$ dollar body 11730 ; semicolon inside $dz$
SELECT [bracket_11731] FROM [dbo].[tbl_11];
INSERT INTO bench_t_84 (id, payload) VALUES (11732, 'v11732');
SELECT `mysql_11733` FROM `tbl_33`;
SELECT [bracket_11734] FROM [dbo].[tbl_14];
UPDATE bench_t_23 SET payload = 11735 WHERE id = 23;
SELECT 11736 AS id, 'row_11736' AS label;
SELECT 11737 AS id, 'row_11737' AS label;
DELETE FROM bench_t_26 WHERE id = 10;
# hash comment 11739
SELECT * FROM "quoted_11740" WHERE col = E'esc\'11740';
WITH cte_11741 AS (SELECT 11741 AS n) SELECT n FROM cte_11741;
WITH cte_11742 AS (SELECT 11742 AS n) SELECT n FROM cte_11742;
SELECT `mysql_11743` FROM `tbl_43`;
SELECT [bracket_11744] FROM [dbo].[tbl_24];
SELECT nested FROM t WHERE id IN (11745, 11746, 11747);
/* block header 11746 */
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
INSERT INTO bench_t_100 (id, payload) VALUES (11748, 'O''Brien');
INSERT INTO bench_t_101 (id, payload) VALUES (11749, 'v11749');
/*
 * section 47
 * checksum d357
 */
$dz$ dollar body 11750 ; semicolon inside $dz$
INSERT INTO bench_t_107 (id, payload) VALUES (11755, 'v11755');
/* block header 11756 */
BEGIN; SELECT 11757; COMMIT;
-- line 11758: deterministic comment
/* block header 11759 */
DELETE FROM bench_t_16 WHERE id = 0;
SELECT nested FROM t WHERE id IN (11761, 11762, 11763);
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT * FROM "quoted_11763" WHERE col = E'esc\'11763';
SELECT nested FROM t WHERE id IN (11764, 11765, 11766);
DELETE FROM bench_t_21 WHERE id = 5;
UPDATE bench_t_54 SET payload = 11766 WHERE id = 22;
DELETE FROM bench_t_23 WHERE id = 7;
BEGIN; SELECT 11768; COMMIT;
-- line 11769: deterministic comment
SELECT * FROM "quoted_11770" WHERE col = E'esc\'11770';
UPDATE bench_t_59 SET payload = 11771 WHERE id = 27;
SELECT 11772 AS id, 'row_11772' AS label;
SELECT 11773 AS id, 'row_11773' AS label;
SELECT [bracket_11774] FROM [dbo].[tbl_14];
WITH cte_11775 AS (SELECT 11775 AS n) SELECT n FROM cte_11775;
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
UPDATE bench_t_1 SET payload = 11777 WHERE id = 1;
SELECT 11778 AS id, 'row_11778' AS label;
# hash comment 11779
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
UPDATE bench_t_5 SET payload = 11781 WHERE id = 5;
WITH cte_11782 AS (SELECT 11782 AS n) SELECT n FROM cte_11782;
DELETE FROM bench_t_7 WHERE id = 7;
# hash comment 11784
SELECT nested FROM t WHERE id IN (11785, 11786, 11787);
SELECT * FROM "quoted_11786" WHERE col = E'esc\'11786';
WITH cte_11787 AS (SELECT 11787 AS n) SELECT n FROM cte_11787;
SELECT [bracket_11788] FROM [dbo].[tbl_28];
SELECT nested FROM t WHERE id IN (11789, 11790, 11791);
DELETE FROM bench_t_14 WHERE id = 14;
SELECT * FROM "quoted_11791" WHERE col = E'esc\'11791';
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT `mysql_11793` FROM `tbl_43`;
BEGIN; SELECT 11794; COMMIT;
INSERT INTO bench_t_19 (id, payload) VALUES (11795, 'v11795');
# hash comment 11796
UPDATE bench_t_21 SET payload = 11797 WHERE id = 21;
WITH cte_11798 AS (SELECT 11798 AS n) SELECT n FROM cte_11798;
INSERT INTO bench_t_23 (id, payload) VALUES (11799, 'v11799');
BEGIN; SELECT 11800; COMMIT;
BEGIN; SELECT 11801; COMMIT;
SELECT 11802 AS id, 'row_11802' AS label;
# hash comment 11803
$dz$ dollar body 11804 ; semicolon inside $dz$
WITH cte_11805 AS (SELECT 11805 AS n) SELECT n FROM cte_11805;
/* block header 11806 */
INSERT INTO bench_t_31 (id, payload) VALUES (11807, 'v11807');
SELECT 11808 AS id, 'row_11808' AS label;
UPDATE bench_t_33 SET payload = 11809 WHERE id = 1;
UPDATE bench_t_34 SET payload = 11810 WHERE id = 2;
# hash comment 11811
INSERT INTO bench_t_36 (id, payload) VALUES (11812, 'v11812');
SELECT nested FROM t WHERE id IN (11813, 11814, 11815);
UPDATE bench_t_38 SET payload = 11814 WHERE id = 6;
$dz$ dollar body 11815 ; semicolon inside $dz$
INSERT INTO bench_t_40 (id, payload) VALUES (11816, 'v11816');
SELECT [bracket_11817] FROM [dbo].[tbl_17];
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT nested FROM t WHERE id IN (11819, 11820, 11821);
/* block header 11820 */
SELECT 11821 AS id, 'row_11821' AS label;
INSERT INTO bench_t_46 (id, payload) VALUES (11822, 'v11822');
-- line 11823: deterministic comment
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
$dz$ dollar body 11825 ; semicolon inside $dz$
/* block header 11826 */
DELETE FROM bench_t_19 WHERE id = 3;
BEGIN; SELECT 11828; COMMIT;
/* block header 11829 */
$dz$ dollar body 11830 ; semicolon inside $dz$
SELECT `mysql_11831` FROM `tbl_31`;
UPDATE bench_t_56 SET payload = 11832 WHERE id = 24;
WITH cte_11833 AS (SELECT 11833 AS n) SELECT n FROM cte_11833;
SELECT nested FROM t WHERE id IN (11834, 11835, 11836);
SELECT * FROM "quoted_11835" WHERE col = E'esc\'11835';
-- line 11836: deterministic comment
SELECT 11837 AS id, 'row_11837' AS label;
INSERT INTO bench_t_62 (id, payload) VALUES (11838, 'v11838');
/* block header 11839 */
INSERT INTO bench_t_64 (id, payload) VALUES (11840, 'v11840');
# hash comment 11841
SELECT [bracket_11842] FROM [dbo].[tbl_2];
$dz$ dollar body 11843 ; semicolon inside $dz$
/* block header 11844 */
DELETE FROM bench_t_5 WHERE id = 5;
UPDATE bench_t_6 SET payload = 11846 WHERE id = 6;
SELECT `mysql_11847` FROM `tbl_47`;
SELECT 11848 AS id, 'row_11848' AS label;
SELECT * FROM "quoted_11849" WHERE col = E'esc\'11849';
WITH cte_11850 AS (SELECT 11850 AS n) SELECT n FROM cte_11850;
SELECT [bracket_11851] FROM [dbo].[tbl_11];
$dz$ dollar body 11852 ; semicolon inside $dz$
/* block header 11853 */
BEGIN; SELECT 11854; COMMIT;
SELECT 11855 AS id, 'row_11855' AS label;
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
DELETE FROM bench_t_17 WHERE id = 1;
INSERT INTO bench_t_82 (id, payload) VALUES (11858, 'O''Brien');
-- line 11859: deterministic comment
SELECT [bracket_11860] FROM [dbo].[tbl_20];
INSERT INTO bench_t_85 (id, payload) VALUES (11861, 'v11861');
# hash comment 11862
# hash comment 11863
# hash comment 11864
SELECT * FROM "quoted_11865" WHERE col = E'esc\'11865';
UPDATE bench_t_26 SET payload = 11866 WHERE id = 26;
-- line 11867: deterministic comment
# hash comment 11868
INSERT INTO bench_t_93 (id, payload) VALUES (11869, 'O''Brien');
UPDATE bench_t_30 SET payload = 11870 WHERE id = 30;
BEGIN; SELECT 11871; COMMIT;
BEGIN; SELECT 11872; COMMIT;
INSERT INTO bench_t_97 (id, payload) VALUES (11873, 'v11873');
SELECT [bracket_11874] FROM [dbo].[tbl_34];
INSERT INTO bench_t_99 (id, payload) VALUES (11875, 'v11875');
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
UPDATE bench_t_37 SET payload = 11877 WHERE id = 5;
BEGIN; SELECT 11878; COMMIT;
SELECT `mysql_11879` FROM `tbl_29`;
# hash comment 11880
UPDATE bench_t_41 SET payload = 11881 WHERE id = 9;
-- line 11882: deterministic comment
SELECT [bracket_11883] FROM [dbo].[tbl_3];
WITH cte_11884 AS (SELECT 11884 AS n) SELECT n FROM cte_11884;
# hash comment 11885
-- line 11886: deterministic comment
INSERT INTO bench_t_111 (id, payload) VALUES (11887, 'v11887');
SELECT [bracket_11888] FROM [dbo].[tbl_8];
SELECT nested FROM t WHERE id IN (11889, 11890, 11891);
SELECT * FROM "quoted_11890" WHERE col = E'esc\'11890';
SELECT * FROM "quoted_11891" WHERE col = E'esc\'11891';
SELECT `mysql_11892` FROM `tbl_42`;
DELETE FROM bench_t_21 WHERE id = 5;
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
/* block header 11895 */
/* block header 11896 */
SELECT nested FROM t WHERE id IN (11897, 11898, 11899);
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
INSERT INTO bench_t_123 (id, payload) VALUES (11899, 'v11899');
SELECT `mysql_11900` FROM `tbl_0`;
SELECT 11901 AS id, 'row_11901' AS label;
SELECT * FROM "quoted_11902" WHERE col = E'esc\'11902';
-- line 11903: deterministic comment
DELETE FROM bench_t_0 WHERE id = 0;
/* block header 11905 */
WITH cte_11906 AS (SELECT 11906 AS n) SELECT n FROM cte_11906;
SELECT * FROM "quoted_11907" WHERE col = E'esc\'11907';
SELECT nested FROM t WHERE id IN (11908, 11909, 11910);
SELECT 11909 AS id, 'row_11909' AS label;
UPDATE bench_t_6 SET payload = 11910 WHERE id = 6;
SELECT * FROM "quoted_11911" WHERE col = E'esc\'11911';
WITH cte_11912 AS (SELECT 11912 AS n) SELECT n FROM cte_11912;
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT * FROM "quoted_11914" WHERE col = E'esc\'11914';
DELETE FROM bench_t_11 WHERE id = 11;
INSERT INTO bench_t_12 (id, payload) VALUES (11916, 'v11916');
# hash comment 11917
UPDATE bench_t_14 SET payload = 11918 WHERE id = 14;
# hash comment 11919
SELECT 11920 AS id, 'row_11920' AS label;
WITH cte_11921 AS (SELECT 11921 AS n) SELECT n FROM cte_11921;
SELECT `mysql_11922` FROM `tbl_22`;
$dz$ dollar body 11923 ; semicolon inside $dz$
SELECT 11924 AS id, 'row_11924' AS label;
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT [bracket_11926] FROM [dbo].[tbl_6];
$dz$ dollar body 11927 ; semicolon inside $dz$
WITH cte_11928 AS (SELECT 11928 AS n) SELECT n FROM cte_11928;
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
WITH cte_11932 AS (SELECT 11932 AS n) SELECT n FROM cte_11932;
WITH cte_11933 AS (SELECT 11933 AS n) SELECT n FROM cte_11933;
WITH cte_11934 AS (SELECT 11934 AS n) SELECT n FROM cte_11934;
/* block header 11935 */
SELECT nested FROM t WHERE id IN (11936, 11937, 11938);
SELECT `mysql_11937` FROM `tbl_37`;
$dz$ dollar body 11938 ; semicolon inside $dz$
BEGIN; SELECT 11939; COMMIT;
SELECT 11940 AS id, 'row_11940' AS label;
SELECT [bracket_11941] FROM [dbo].[tbl_21];
SELECT 11942 AS id, 'row_11942' AS label;
INSERT INTO bench_t_39 (id, payload) VALUES (11943, 'v11943');
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT nested FROM t WHERE id IN (11945, 11946, 11947);
SELECT `mysql_11946` FROM `tbl_46`;
WITH cte_11947 AS (SELECT 11947 AS n) SELECT n FROM cte_11947;
SELECT 11948 AS id, 'row_11948' AS label;
-- line 11949: deterministic comment
WITH cte_11950 AS (SELECT 11950 AS n) SELECT n FROM cte_11950;
SELECT nested FROM t WHERE id IN (11951, 11952, 11953);
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT [bracket_11953] FROM [dbo].[tbl_33];
WITH cte_11954 AS (SELECT 11954 AS n) SELECT n FROM cte_11954;
SELECT * FROM "quoted_11955" WHERE col = E'esc\'11955';
SELECT * FROM "quoted_11956" WHERE col = E'esc\'11956';
UPDATE bench_t_53 SET payload = 11957 WHERE id = 21;
-- line 11958: deterministic comment
SELECT * FROM "quoted_11959" WHERE col = E'esc\'11959';
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT * FROM "quoted_11961" WHERE col = E'esc\'11961';
$dz$ dollar body 11962 ; semicolon inside $dz$
BEGIN; SELECT 11963; COMMIT;
/* block header 11964 */
-- line 11965: deterministic comment
DELETE FROM bench_t_30 WHERE id = 14;
BEGIN; SELECT 11967; COMMIT;
SELECT * FROM "quoted_11968" WHERE col = E'esc\'11968';
SELECT * FROM "quoted_11969" WHERE col = E'esc\'11969';
SELECT [bracket_11970] FROM [dbo].[tbl_10];
SELECT [bracket_11971] FROM [dbo].[tbl_11];
DELETE FROM bench_t_4 WHERE id = 4;
BEGIN; SELECT 11973; COMMIT;
WITH cte_11974 AS (SELECT 11974 AS n) SELECT n FROM cte_11974;
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT 11976 AS id, 'row_11976' AS label;
SELECT nested FROM t WHERE id IN (11977, 11978, 11979);
# hash comment 11978
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
DELETE FROM bench_t_12 WHERE id = 12;
-- line 11981: deterministic comment
SELECT * FROM "quoted_11982" WHERE col = E'esc\'11982';
BEGIN; SELECT 11983; COMMIT;
UPDATE bench_t_16 SET payload = 11984 WHERE id = 16;
SELECT [bracket_11985] FROM [dbo].[tbl_25];
SELECT [bracket_11986] FROM [dbo].[tbl_26];
SELECT nested FROM t WHERE id IN (11987, 11988, 11989);
-- line 11988: deterministic comment
UPDATE bench_t_21 SET payload = 11989 WHERE id = 21;
WITH cte_11990 AS (SELECT 11990 AS n) SELECT n FROM cte_11990;
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
DELETE FROM bench_t_24 WHERE id = 8;
/* block header 11993 */
WITH cte_11994 AS (SELECT 11994 AS n) SELECT n FROM cte_11994;
UPDATE bench_t_27 SET payload = 11995 WHERE id = 27;
SELECT [bracket_11996] FROM [dbo].[tbl_36];
/* block header 11997 */
-- line 11998: deterministic comment
$dz$ dollar body 11999 ; semicolon inside $dz$
/*
 * section 48
 * checksum 94d8
 */
-- line 12000: deterministic comment
-- line 12005: deterministic comment
WITH cte_12006 AS (SELECT 12006 AS n) SELECT n FROM cte_12006;
INSERT INTO bench_t_103 (id, payload) VALUES (12007, 'v12007');
INSERT INTO bench_t_104 (id, payload) VALUES (12008, 'v12008');
UPDATE bench_t_41 SET payload = 12009 WHERE id = 9;
UPDATE bench_t_42 SET payload = 12010 WHERE id = 10;
DELETE FROM bench_t_11 WHERE id = 11;
SELECT `mysql_12012` FROM `tbl_12`;
BEGIN; SELECT 12013; COMMIT;
/* block header 12014 */
-- line 12015: deterministic comment
WITH cte_12016 AS (SELECT 12016 AS n) SELECT n FROM cte_12016;
# hash comment 12017
DELETE FROM bench_t_18 WHERE id = 2;
$dz$ dollar body 12019 ; semicolon inside $dz$
BEGIN; SELECT 12020; COMMIT;
INSERT INTO bench_t_117 (id, payload) VALUES (12021, 'v12021');
SELECT [bracket_12022] FROM [dbo].[tbl_22];
SELECT 12023 AS id, 'row_12023' AS label;
$dz$ dollar body 12024 ; semicolon inside $dz$
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
UPDATE bench_t_58 SET payload = 12026 WHERE id = 26;
SELECT `mysql_12027` FROM `tbl_27`;
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
INSERT INTO bench_t_125 (id, payload) VALUES (12029, 'v12029');
BEGIN; SELECT 12030; COMMIT;
# hash comment 12031
SELECT 12032 AS id, 'row_12032' AS label;
DELETE FROM bench_t_1 WHERE id = 1;
BEGIN; SELECT 12034; COMMIT;
/* block header 12035 */
# hash comment 12036
/* block header 12037 */
# hash comment 12038
BEGIN; SELECT 12039; COMMIT;
$dz$ dollar body 12040 ; semicolon inside $dz$
WITH cte_12041 AS (SELECT 12041 AS n) SELECT n FROM cte_12041;
/* block header 12042 */
SELECT 12043 AS id, 'row_12043' AS label;
INSERT INTO bench_t_12 (id, payload) VALUES (12044, 'v12044');
SELECT `mysql_12045` FROM `tbl_45`;
# hash comment 12046
INSERT INTO bench_t_15 (id, payload) VALUES (12047, 'v12047');
# hash comment 12048
SELECT * FROM "quoted_12049" WHERE col = E'esc\'12049';
BEGIN; SELECT 12050; COMMIT;
UPDATE bench_t_19 SET payload = 12051 WHERE id = 19;
UPDATE bench_t_20 SET payload = 12052 WHERE id = 20;
DELETE FROM bench_t_21 WHERE id = 5;
/* block header 12054 */
UPDATE bench_t_23 SET payload = 12055 WHERE id = 23;
$dz$ dollar body 12056 ; semicolon inside $dz$
INSERT INTO bench_t_25 (id, payload) VALUES (12057, 'v12057');
SELECT `mysql_12058` FROM `tbl_8`;
# hash comment 12059
BEGIN; SELECT 12060; COMMIT;
SELECT * FROM "quoted_12061" WHERE col = E'esc\'12061';
INSERT INTO bench_t_30 (id, payload) VALUES (12062, 'v12062');
# hash comment 12063
# hash comment 12064
UPDATE bench_t_33 SET payload = 12065 WHERE id = 1;
SELECT * FROM "quoted_12066" WHERE col = E'esc\'12066';
SELECT [bracket_12067] FROM [dbo].[tbl_27];
BEGIN; SELECT 12068; COMMIT;
SELECT [bracket_12069] FROM [dbo].[tbl_29];
INSERT INTO bench_t_38 (id, payload) VALUES (12070, 'v12070');
INSERT INTO bench_t_39 (id, payload) VALUES (12071, 'v12071');
SELECT 12072 AS id, 'row_12072' AS label;
SELECT `mysql_12073` FROM `tbl_23`;
WITH cte_12074 AS (SELECT 12074 AS n) SELECT n FROM cte_12074;
# hash comment 12075
SELECT `mysql_12076` FROM `tbl_26`;
# hash comment 12077
/* block header 12078 */
/* block header 12079 */
INSERT INTO bench_t_48 (id, payload) VALUES (12080, 'v12080');
SELECT 12081 AS id, 'row_12081' AS label;
UPDATE bench_t_50 SET payload = 12082 WHERE id = 18;
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT `mysql_12084` FROM `tbl_34`;
# hash comment 12085
/* block header 12086 */
SELECT nested FROM t WHERE id IN (12087, 12088, 12089);
/* block header 12088 */
WITH cte_12089 AS (SELECT 12089 AS n) SELECT n FROM cte_12089;
INSERT INTO bench_t_58 (id, payload) VALUES (12090, 'v12090');
-- line 12091: deterministic comment
INSERT INTO bench_t_60 (id, payload) VALUES (12092, 'v12092');
SELECT `mysql_12093` FROM `tbl_43`;
SELECT nested FROM t WHERE id IN (12094, 12095, 12096);
WITH cte_12095 AS (SELECT 12095 AS n) SELECT n FROM cte_12095;
SELECT [bracket_12096] FROM [dbo].[tbl_16];
-- line 12097: deterministic comment
SELECT 12098 AS id, 'row_12098' AS label;
UPDATE bench_t_3 SET payload = 12099 WHERE id = 3;
SELECT nested FROM t WHERE id IN (12100, 12101, 12102);
SELECT 12101 AS id, 'row_12101' AS label;
/* block header 12102 */
SELECT [bracket_12103] FROM [dbo].[tbl_23];
-- line 12104: deterministic comment
# hash comment 12105
SELECT `mysql_12106` FROM `tbl_6`;
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
UPDATE bench_t_12 SET payload = 12108 WHERE id = 12;
UPDATE bench_t_13 SET payload = 12109 WHERE id = 13;
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
INSERT INTO bench_t_79 (id, payload) VALUES (12111, 'O''Brien');
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
WITH cte_12113 AS (SELECT 12113 AS n) SELECT n FROM cte_12113;
SELECT [bracket_12114] FROM [dbo].[tbl_34];
INSERT INTO bench_t_83 (id, payload) VALUES (12115, 'v12115');
SELECT [bracket_12116] FROM [dbo].[tbl_36];
SELECT 12117 AS id, 'row_12117' AS label;
BEGIN; SELECT 12118; COMMIT;
SELECT nested FROM t WHERE id IN (12119, 12120, 12121);
$dz$ dollar body 12120 ; semicolon inside $dz$
SELECT [bracket_12121] FROM [dbo].[tbl_1];
UPDATE bench_t_26 SET payload = 12122 WHERE id = 26;
SELECT `mysql_12123` FROM `tbl_23`;
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
$dz$ dollar body 12125 ; semicolon inside $dz$
INSERT INTO bench_t_94 (id, payload) VALUES (12126, 'v12126');
SELECT * FROM "quoted_12127" WHERE col = E'esc\'12127';
UPDATE bench_t_32 SET payload = 12128 WHERE id = 0;
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
# hash comment 12130
# hash comment 12131
/* block header 12132 */
INSERT INTO bench_t_101 (id, payload) VALUES (12133, 'O''Brien');
$dz$ dollar body 12134 ; semicolon inside $dz$
WITH cte_12135 AS (SELECT 12135 AS n) SELECT n FROM cte_12135;
UPDATE bench_t_40 SET payload = 12136 WHERE id = 8;
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
DELETE FROM bench_t_10 WHERE id = 10;
SELECT nested FROM t WHERE id IN (12139, 12140, 12141);
SELECT [bracket_12140] FROM [dbo].[tbl_20];
DELETE FROM bench_t_13 WHERE id = 13;
SELECT nested FROM t WHERE id IN (12142, 12143, 12144);
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
INSERT INTO bench_t_112 (id, payload) VALUES (12144, 'O''Brien');
SELECT [bracket_12145] FROM [dbo].[tbl_25];
SELECT [bracket_12146] FROM [dbo].[tbl_26];
SELECT nested FROM t WHERE id IN (12147, 12148, 12149);
SELECT nested FROM t WHERE id IN (12148, 12149, 12150);
/* block header 12149 */
# hash comment 12150
WITH cte_12151 AS (SELECT 12151 AS n) SELECT n FROM cte_12151;
-- line 12152: deterministic comment
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
# hash comment 12154
SELECT * FROM "quoted_12155" WHERE col = E'esc\'12155';
$dz$ dollar body 12156 ; semicolon inside $dz$
# hash comment 12157
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
/* block header 12159 */
/* block header 12160 */
SELECT 12161 AS id, 'row_12161' AS label;
BEGIN; SELECT 12162; COMMIT;
BEGIN; SELECT 12163; COMMIT;
SELECT * FROM "quoted_12164" WHERE col = E'esc\'12164';
SELECT nested FROM t WHERE id IN (12165, 12166, 12167);
WITH cte_12166 AS (SELECT 12166 AS n) SELECT n FROM cte_12166;
/* block header 12167 */
DELETE FROM bench_t_8 WHERE id = 8;
SELECT nested FROM t WHERE id IN (12169, 12170, 12171);
SELECT 12170 AS id, 'row_12170' AS label;
INSERT INTO bench_t_11 (id, payload) VALUES (12171, 'v12171');
SELECT nested FROM t WHERE id IN (12172, 12173, 12174);
UPDATE bench_t_13 SET payload = 12173 WHERE id = 13;
-- line 12174: deterministic comment
/* block header 12175 */
SELECT `mysql_12176` FROM `tbl_26`;
BEGIN; SELECT 12177; COMMIT;
WITH cte_12178 AS (SELECT 12178 AS n) SELECT n FROM cte_12178;
SELECT [bracket_12179] FROM [dbo].[tbl_19];
SELECT 12180 AS id, 'row_12180' AS label;
DELETE FROM bench_t_21 WHERE id = 5;
-- line 12182: deterministic comment
UPDATE bench_t_23 SET payload = 12183 WHERE id = 23;
$dz$ dollar body 12184 ; semicolon inside $dz$
/* block header 12185 */
-- line 12186: deterministic comment
SELECT nested FROM t WHERE id IN (12187, 12188, 12189);
SELECT `mysql_12188` FROM `tbl_38`;
/* block header 12189 */
SELECT `mysql_12190` FROM `tbl_40`;
INSERT INTO bench_t_31 (id, payload) VALUES (12191, 'v12191');
UPDATE bench_t_32 SET payload = 12192 WHERE id = 0;
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT * FROM "quoted_12194" WHERE col = E'esc\'12194';
WITH cte_12195 AS (SELECT 12195 AS n) SELECT n FROM cte_12195;
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
DELETE FROM bench_t_5 WHERE id = 5;
INSERT INTO bench_t_38 (id, payload) VALUES (12198, 'v12198');
/* block header 12199 */
SELECT 12200 AS id, 'row_12200' AS label;
SELECT `mysql_12201` FROM `tbl_1`;
WITH cte_12202 AS (SELECT 12202 AS n) SELECT n FROM cte_12202;
SELECT * FROM "quoted_12203" WHERE col = E'esc\'12203';
WITH cte_12204 AS (SELECT 12204 AS n) SELECT n FROM cte_12204;
DELETE FROM bench_t_13 WHERE id = 13;
SELECT 12206 AS id, 'row_12206' AS label;
-- line 12207: deterministic comment
SELECT * FROM "quoted_12208" WHERE col = E'esc\'12208';
SELECT `mysql_12209` FROM `tbl_9`;
INSERT INTO bench_t_50 (id, payload) VALUES (12210, 'O''Brien');
SELECT 12211 AS id, 'row_12211' AS label;
SELECT 12212 AS id, 'row_12212' AS label;
SELECT * FROM "quoted_12213" WHERE col = E'esc\'12213';
SELECT [bracket_12214] FROM [dbo].[tbl_14];
DELETE FROM bench_t_23 WHERE id = 7;
SELECT [bracket_12216] FROM [dbo].[tbl_16];
/* block header 12217 */
SELECT 12218 AS id, 'row_12218' AS label;
# hash comment 12219
BEGIN; SELECT 12220; COMMIT;
$dz$ dollar body 12221 ; semicolon inside $dz$
WITH cte_12222 AS (SELECT 12222 AS n) SELECT n FROM cte_12222;
SELECT nested FROM t WHERE id IN (12223, 12224, 12225);
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT 12226 AS id, 'row_12226' AS label;
SELECT `mysql_12227` FROM `tbl_27`;
SELECT * FROM "quoted_12228" WHERE col = E'esc\'12228';
SELECT `mysql_12229` FROM `tbl_29`;
BEGIN; SELECT 12230; COMMIT;
/* block header 12231 */
BEGIN; SELECT 12232; COMMIT;
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
DELETE FROM bench_t_10 WHERE id = 10;
-- line 12235: deterministic comment
/* block header 12236 */
# hash comment 12237
DELETE FROM bench_t_14 WHERE id = 14;
BEGIN; SELECT 12239; COMMIT;
-- line 12240: deterministic comment
-- line 12241: deterministic comment
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
BEGIN; SELECT 12243; COMMIT;
-- line 12244: deterministic comment
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
DELETE FROM bench_t_22 WHERE id = 6;
-- line 12247: deterministic comment
SELECT 12248 AS id, 'row_12248' AS label;
SELECT nested FROM t WHERE id IN (12249, 12250, 12251);
/*
 * section 49
 * checksum 9676
 */
$dz$ dollar body 12250 ; semicolon inside $dz$
UPDATE bench_t_31 SET payload = 12255 WHERE id = 31;
/* block header 12256 */
/* block header 12257 */
$dz$ dollar body 12258 ; semicolon inside $dz$
# hash comment 12259
/* block header 12260 */
SELECT 12261 AS id, 'row_12261' AS label;
SELECT 12262 AS id, 'row_12262' AS label;
-- line 12263: deterministic comment
SELECT `mysql_12264` FROM `tbl_14`;
SELECT [bracket_12265] FROM [dbo].[tbl_25];
SELECT [bracket_12266] FROM [dbo].[tbl_26];
BEGIN; SELECT 12267; COMMIT;
UPDATE bench_t_44 SET payload = 12268 WHERE id = 12;
DELETE FROM bench_t_13 WHERE id = 13;
SELECT `mysql_12270` FROM `tbl_20`;
SELECT 12271 AS id, 'row_12271' AS label;
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT [bracket_12273] FROM [dbo].[tbl_33];
$dz$ dollar body 12274 ; semicolon inside $dz$
-- line 12275: deterministic comment
SELECT `mysql_12276` FROM `tbl_26`;
WITH cte_12277 AS (SELECT 12277 AS n) SELECT n FROM cte_12277;
$dz$ dollar body 12278 ; semicolon inside $dz$
DELETE FROM bench_t_23 WHERE id = 7;
BEGIN; SELECT 12280; COMMIT;
DELETE FROM bench_t_25 WHERE id = 9;
/* block header 12282 */
DELETE FROM bench_t_27 WHERE id = 11;
UPDATE bench_t_60 SET payload = 12284 WHERE id = 28;
SELECT 12285 AS id, 'row_12285' AS label;
$dz$ dollar body 12286 ; semicolon inside $dz$
SELECT [bracket_12287] FROM [dbo].[tbl_7];
WITH cte_12288 AS (SELECT 12288 AS n) SELECT n FROM cte_12288;
-- line 12289: deterministic comment
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT [bracket_12291] FROM [dbo].[tbl_11];
UPDATE bench_t_4 SET payload = 12292 WHERE id = 4;
SELECT * FROM "quoted_12293" WHERE col = E'esc\'12293';
SELECT [bracket_12294] FROM [dbo].[tbl_14];
UPDATE bench_t_7 SET payload = 12295 WHERE id = 7;
SELECT 12296 AS id, 'row_12296' AS label;
SELECT [bracket_12297] FROM [dbo].[tbl_17];
SELECT 12298 AS id, 'row_12298' AS label;
SELECT [bracket_12299] FROM [dbo].[tbl_19];
WITH cte_12300 AS (SELECT 12300 AS n) SELECT n FROM cte_12300;
-- line 12301: deterministic comment
BEGIN; SELECT 12302; COMMIT;
UPDATE bench_t_15 SET payload = 12303 WHERE id = 15;
SELECT * FROM "quoted_12304" WHERE col = E'esc\'12304';
WITH cte_12305 AS (SELECT 12305 AS n) SELECT n FROM cte_12305;
# hash comment 12306
SELECT `mysql_12307` FROM `tbl_7`;
SELECT 12308 AS id, 'row_12308' AS label;
SELECT 12309 AS id, 'row_12309' AS label;
UPDATE bench_t_22 SET payload = 12310 WHERE id = 22;
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
DELETE FROM bench_t_24 WHERE id = 8;
INSERT INTO bench_t_25 (id, payload) VALUES (12313, 'v12313');
-- line 12314: deterministic comment
# hash comment 12315
# hash comment 12316
DELETE FROM bench_t_29 WHERE id = 13;
SELECT * FROM "quoted_12318" WHERE col = E'esc\'12318';
SELECT [bracket_12319] FROM [dbo].[tbl_39];
-- line 12320: deterministic comment
# hash comment 12321
-- line 12322: deterministic comment
/* block header 12323 */
# hash comment 12324
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
INSERT INTO bench_t_38 (id, payload) VALUES (12326, 'v12326');
UPDATE bench_t_39 SET payload = 12327 WHERE id = 7;
SELECT * FROM "quoted_12328" WHERE col = E'esc\'12328';
SELECT * FROM "quoted_12329" WHERE col = E'esc\'12329';
/* block header 12330 */
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT 12332 AS id, 'row_12332' AS label;
SELECT 12333 AS id, 'row_12333' AS label;
SELECT `mysql_12334` FROM `tbl_34`;
/* block header 12335 */
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
DELETE FROM bench_t_17 WHERE id = 1;
DELETE FROM bench_t_18 WHERE id = 2;
INSERT INTO bench_t_51 (id, payload) VALUES (12339, 'v12339');
INSERT INTO bench_t_52 (id, payload) VALUES (12340, 'v12340');
$dz$ dollar body 12341 ; semicolon inside $dz$
SELECT 12342 AS id, 'row_12342' AS label;
INSERT INTO bench_t_55 (id, payload) VALUES (12343, 'v12343');
SELECT * FROM "quoted_12344" WHERE col = E'esc\'12344';
WITH cte_12345 AS (SELECT 12345 AS n) SELECT n FROM cte_12345;
SELECT [bracket_12346] FROM [dbo].[tbl_26];
DELETE FROM bench_t_27 WHERE id = 11;
# hash comment 12348
UPDATE bench_t_61 SET payload = 12349 WHERE id = 29;
SELECT [bracket_12350] FROM [dbo].[tbl_30];
# hash comment 12351
-- line 12352: deterministic comment
SELECT `mysql_12353` FROM `tbl_3`;
BEGIN; SELECT 12354; COMMIT;
# hash comment 12355
DELETE FROM bench_t_4 WHERE id = 4;
UPDATE bench_t_5 SET payload = 12357 WHERE id = 5;
SELECT nested FROM t WHERE id IN (12358, 12359, 12360);
DELETE FROM bench_t_7 WHERE id = 7;
/* block header 12360 */
SELECT [bracket_12361] FROM [dbo].[tbl_1];
WITH cte_12362 AS (SELECT 12362 AS n) SELECT n FROM cte_12362;
SELECT 12363 AS id, 'row_12363' AS label;
UPDATE bench_t_12 SET payload = 12364 WHERE id = 12;
SELECT 12365 AS id, 'row_12365' AS label;
-- line 12366: deterministic comment
BEGIN; SELECT 12367; COMMIT;
SELECT nested FROM t WHERE id IN (12368, 12369, 12370);
DELETE FROM bench_t_17 WHERE id = 1;
INSERT INTO bench_t_82 (id, payload) VALUES (12370, 'v12370');
DELETE FROM bench_t_19 WHERE id = 3;
SELECT [bracket_12372] FROM [dbo].[tbl_12];
SELECT `mysql_12373` FROM `tbl_23`;
WITH cte_12374 AS (SELECT 12374 AS n) SELECT n FROM cte_12374;
SELECT nested FROM t WHERE id IN (12375, 12376, 12377);
UPDATE bench_t_24 SET payload = 12376 WHERE id = 24;
/* block header 12377 */
DELETE FROM bench_t_26 WHERE id = 10;
WITH cte_12379 AS (SELECT 12379 AS n) SELECT n FROM cte_12379;
UPDATE bench_t_28 SET payload = 12380 WHERE id = 28;
SELECT nested FROM t WHERE id IN (12381, 12382, 12383);
BEGIN; SELECT 12382; COMMIT;
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
DELETE FROM bench_t_0 WHERE id = 0;
DELETE FROM bench_t_1 WHERE id = 1;
UPDATE bench_t_34 SET payload = 12386 WHERE id = 2;
# hash comment 12387
BEGIN; SELECT 12388; COMMIT;
# hash comment 12389
$dz$ dollar body 12390 ; semicolon inside $dz$
INSERT INTO bench_t_103 (id, payload) VALUES (12391, 'v12391');
INSERT INTO bench_t_104 (id, payload) VALUES (12392, 'v12392');
-- line 12393: deterministic comment
UPDATE bench_t_42 SET payload = 12394 WHERE id = 10;
SELECT [bracket_12395] FROM [dbo].[tbl_35];
BEGIN; SELECT 12396; COMMIT;
BEGIN; SELECT 12397; COMMIT;
/* block header 12398 */
SELECT [bracket_12399] FROM [dbo].[tbl_39];
# hash comment 12400
SELECT nested FROM t WHERE id IN (12401, 12402, 12403);
UPDATE bench_t_50 SET payload = 12402 WHERE id = 18;
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
# hash comment 12404
# hash comment 12405
/* block header 12406 */
UPDATE bench_t_55 SET payload = 12407 WHERE id = 23;
SELECT [bracket_12408] FROM [dbo].[tbl_8];
/* block header 12409 */
SELECT * FROM "quoted_12410" WHERE col = E'esc\'12410';
WITH cte_12411 AS (SELECT 12411 AS n) SELECT n FROM cte_12411;
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
UPDATE bench_t_61 SET payload = 12413 WHERE id = 29;
SELECT [bracket_12414] FROM [dbo].[tbl_14];
SELECT [bracket_12415] FROM [dbo].[tbl_15];
SELECT nested FROM t WHERE id IN (12416, 12417, 12418);
-- line 12417: deterministic comment
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
UPDATE bench_t_3 SET payload = 12419 WHERE id = 3;
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
BEGIN; SELECT 12421; COMMIT;
WITH cte_12422 AS (SELECT 12422 AS n) SELECT n FROM cte_12422;
SELECT [bracket_12423] FROM [dbo].[tbl_23];
INSERT INTO bench_t_8 (id, payload) VALUES (12424, 'v12424');
-- line 12425: deterministic comment
SELECT 12426 AS id, 'row_12426' AS label;
WITH cte_12427 AS (SELECT 12427 AS n) SELECT n FROM cte_12427;
UPDATE bench_t_12 SET payload = 12428 WHERE id = 12;
WITH cte_12429 AS (SELECT 12429 AS n) SELECT n FROM cte_12429;
SELECT * FROM "quoted_12430" WHERE col = E'esc\'12430';
SELECT nested FROM t WHERE id IN (12431, 12432, 12433);
SELECT * FROM "quoted_12432" WHERE col = E'esc\'12432';
WITH cte_12433 AS (SELECT 12433 AS n) SELECT n FROM cte_12433;
# hash comment 12434
DELETE FROM bench_t_19 WHERE id = 3;
SELECT 12436 AS id, 'row_12436' AS label;
-- line 12437: deterministic comment
WITH cte_12438 AS (SELECT 12438 AS n) SELECT n FROM cte_12438;
/* block header 12439 */
/* block header 12440 */
BEGIN; SELECT 12441; COMMIT;
SELECT * FROM "quoted_12442" WHERE col = E'esc\'12442';
BEGIN; SELECT 12443; COMMIT;
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
# hash comment 12445
BEGIN; SELECT 12446; COMMIT;
BEGIN; SELECT 12447; COMMIT;
/* block header 12448 */
WITH cte_12449 AS (SELECT 12449 AS n) SELECT n FROM cte_12449;
DELETE FROM bench_t_2 WHERE id = 2;
SELECT [bracket_12451] FROM [dbo].[tbl_11];
/* block header 12452 */
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT nested FROM t WHERE id IN (12454, 12455, 12456);
WITH cte_12455 AS (SELECT 12455 AS n) SELECT n FROM cte_12455;
/* block header 12456 */
SELECT [bracket_12457] FROM [dbo].[tbl_17];
WITH cte_12458 AS (SELECT 12458 AS n) SELECT n FROM cte_12458;
SELECT [bracket_12459] FROM [dbo].[tbl_19];
UPDATE bench_t_44 SET payload = 12460 WHERE id = 12;
DELETE FROM bench_t_13 WHERE id = 13;
INSERT INTO bench_t_46 (id, payload) VALUES (12462, 'v12462');
UPDATE bench_t_47 SET payload = 12463 WHERE id = 15;
$dz$ dollar body 12464 ; semicolon inside $dz$
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
# hash comment 12466
UPDATE bench_t_51 SET payload = 12467 WHERE id = 19;
SELECT `mysql_12468` FROM `tbl_18`;
SELECT * FROM "quoted_12469" WHERE col = E'esc\'12469';
SELECT nested FROM t WHERE id IN (12470, 12471, 12472);
SELECT [bracket_12471] FROM [dbo].[tbl_31];
SELECT [bracket_12472] FROM [dbo].[tbl_32];
WITH cte_12473 AS (SELECT 12473 AS n) SELECT n FROM cte_12473;
$dz$ dollar body 12474 ; semicolon inside $dz$
SELECT [bracket_12475] FROM [dbo].[tbl_35];
UPDATE bench_t_60 SET payload = 12476 WHERE id = 28;
BEGIN; SELECT 12477; COMMIT;
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
WITH cte_12479 AS (SELECT 12479 AS n) SELECT n FROM cte_12479;
# hash comment 12480
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
BEGIN; SELECT 12482; COMMIT;
# hash comment 12483
$dz$ dollar body 12484 ; semicolon inside $dz$
/* block header 12485 */
INSERT INTO bench_t_70 (id, payload) VALUES (12486, 'v12486');
BEGIN; SELECT 12487; COMMIT;
DELETE FROM bench_t_8 WHERE id = 8;
SELECT `mysql_12489` FROM `tbl_39`;
SELECT `mysql_12490` FROM `tbl_40`;
# hash comment 12491
SELECT [bracket_12492] FROM [dbo].[tbl_12];
DELETE FROM bench_t_13 WHERE id = 13;
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
/* block header 12495 */
INSERT INTO bench_t_80 (id, payload) VALUES (12496, 'O''Brien');
SELECT [bracket_12497] FROM [dbo].[tbl_17];
SELECT [bracket_12498] FROM [dbo].[tbl_18];
SELECT `mysql_12499` FROM `tbl_49`;
/*
 * section 50
 * checksum 196b
 */
SELECT [bracket_12500] FROM [dbo].[tbl_20];
# hash comment 12505
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
WITH cte_12507 AS (SELECT 12507 AS n) SELECT n FROM cte_12507;
SELECT `mysql_12508` FROM `tbl_8`;
# hash comment 12509
SELECT 12510 AS id, 'row_12510' AS label;
SELECT 12511 AS id, 'row_12511' AS label;
UPDATE bench_t_32 SET payload = 12512 WHERE id = 0;
UPDATE bench_t_33 SET payload = 12513 WHERE id = 1;
/* block header 12514 */
SELECT * FROM "quoted_12515" WHERE col = E'esc\'12515';
/* block header 12516 */
SELECT [bracket_12517] FROM [dbo].[tbl_37];
SELECT * FROM "quoted_12518" WHERE col = E'esc\'12518';
BEGIN; SELECT 12519; COMMIT;
SELECT `mysql_12520` FROM `tbl_20`;
SELECT 12521 AS id, 'row_12521' AS label;
BEGIN; SELECT 12522; COMMIT;
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
$dz$ dollar body 12524 ; semicolon inside $dz$
UPDATE bench_t_45 SET payload = 12525 WHERE id = 13;
SELECT * FROM "quoted_12526" WHERE col = E'esc\'12526';
$dz$ dollar body 12527 ; semicolon inside $dz$
SELECT nested FROM t WHERE id IN (12528, 12529, 12530);
BEGIN; SELECT 12529; COMMIT;
SELECT `mysql_12530` FROM `tbl_30`;
SELECT 12531 AS id, 'row_12531' AS label;
# hash comment 12532
SELECT * FROM "quoted_12533" WHERE col = E'esc\'12533';
BEGIN; SELECT 12534; COMMIT;
# hash comment 12535
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
WITH cte_12537 AS (SELECT 12537 AS n) SELECT n FROM cte_12537;
UPDATE bench_t_58 SET payload = 12538 WHERE id = 26;
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
DELETE FROM bench_t_28 WHERE id = 12;
SELECT * FROM "quoted_12541" WHERE col = E'esc\'12541';
WITH cte_12542 AS (SELECT 12542 AS n) SELECT n FROM cte_12542;
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT `mysql_12544` FROM `tbl_44`;
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT [bracket_12546] FROM [dbo].[tbl_26];
SELECT [bracket_12547] FROM [dbo].[tbl_27];
SELECT 12548 AS id, 'row_12548' AS label;
DELETE FROM bench_t_5 WHERE id = 5;
SELECT [bracket_12550] FROM [dbo].[tbl_30];
INSERT INTO bench_t_7 (id, payload) VALUES (12551, 'O''Brien');
# hash comment 12552
INSERT INTO bench_t_9 (id, payload) VALUES (12553, 'v12553');
WITH cte_12554 AS (SELECT 12554 AS n) SELECT n FROM cte_12554;
$dz$ dollar body 12555 ; semicolon inside $dz$
$dz$ dollar body 12556 ; semicolon inside $dz$
SELECT `mysql_12557` FROM `tbl_7`;
SELECT 12558 AS id, 'row_12558' AS label;
SELECT `mysql_12559` FROM `tbl_9`;
WITH cte_12560 AS (SELECT 12560 AS n) SELECT n FROM cte_12560;
WITH cte_12561 AS (SELECT 12561 AS n) SELECT n FROM cte_12561;
/* block header 12562 */
SELECT nested FROM t WHERE id IN (12563, 12564, 12565);
# hash comment 12564
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
# hash comment 12566
/* block header 12567 */
SELECT * FROM "quoted_12568" WHERE col = E'esc\'12568';
-- line 12569: deterministic comment
BEGIN; SELECT 12570; COMMIT;
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT 12572 AS id, 'row_12572' AS label;
DELETE FROM bench_t_29 WHERE id = 13;
BEGIN; SELECT 12574; COMMIT;
DELETE FROM bench_t_31 WHERE id = 15;
SELECT `mysql_12576` FROM `tbl_26`;
-- line 12577: deterministic comment
# hash comment 12578
$dz$ dollar body 12579 ; semicolon inside $dz$
UPDATE bench_t_36 SET payload = 12580 WHERE id = 4;
SELECT * FROM "quoted_12581" WHERE col = E'esc\'12581';
$dz$ dollar body 12582 ; semicolon inside $dz$
SELECT `mysql_12583` FROM `tbl_33`;
BEGIN; SELECT 12584; COMMIT;
SELECT `mysql_12585` FROM `tbl_35`;
UPDATE bench_t_42 SET payload = 12586 WHERE id = 10;
# hash comment 12587
SELECT 12588 AS id, 'row_12588' AS label;
INSERT INTO bench_t_45 (id, payload) VALUES (12589, 'v12589');
SELECT nested FROM t WHERE id IN (12590, 12591, 12592);
BEGIN; SELECT 12591; COMMIT;
DELETE FROM bench_t_16 WHERE id = 0;
$dz$ dollar body 12593 ; semicolon inside $dz$
WITH cte_12594 AS (SELECT 12594 AS n) SELECT n FROM cte_12594;
SELECT * FROM "quoted_12595" WHERE col = E'esc\'12595';
/* block header 12596 */
-- line 12597: deterministic comment
/* block header 12598 */
SELECT [bracket_12599] FROM [dbo].[tbl_39];
SELECT [bracket_12600] FROM [dbo].[tbl_0];
BEGIN; SELECT 12601; COMMIT;
BEGIN; SELECT 12602; COMMIT;
# hash comment 12603
SELECT [bracket_12604] FROM [dbo].[tbl_4];
UPDATE bench_t_61 SET payload = 12605 WHERE id = 29;
BEGIN; SELECT 12606; COMMIT;
SELECT nested FROM t WHERE id IN (12607, 12608, 12609);
UPDATE bench_t_0 SET payload = 12608 WHERE id = 0;
SELECT 12609 AS id, 'row_12609' AS label;
$dz$ dollar body 12610 ; semicolon inside $dz$
/* block header 12611 */
WITH cte_12612 AS (SELECT 12612 AS n) SELECT n FROM cte_12612;
SELECT 12613 AS id, 'row_12613' AS label;
BEGIN; SELECT 12614; COMMIT;
/* block header 12615 */
/* block header 12616 */
DELETE FROM bench_t_9 WHERE id = 9;
DELETE FROM bench_t_10 WHERE id = 10;
SELECT `mysql_12619` FROM `tbl_19`;
SELECT * FROM "quoted_12620" WHERE col = E'esc\'12620';
BEGIN; SELECT 12621; COMMIT;
SELECT `mysql_12622` FROM `tbl_22`;
SELECT `mysql_12623` FROM `tbl_23`;
# hash comment 12624
UPDATE bench_t_17 SET payload = 12625 WHERE id = 17;
DELETE FROM bench_t_18 WHERE id = 2;
DELETE FROM bench_t_19 WHERE id = 3;
# hash comment 12628
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
BEGIN; SELECT 12630; COMMIT;
INSERT INTO bench_t_87 (id, payload) VALUES (12631, 'v12631');
SELECT [bracket_12632] FROM [dbo].[tbl_32];
SELECT [bracket_12633] FROM [dbo].[tbl_33];
SELECT `mysql_12634` FROM `tbl_34`;
$dz$ dollar body 12635 ; semicolon inside $dz$
UPDATE bench_t_28 SET payload = 12636 WHERE id = 28;
INSERT INTO bench_t_93 (id, payload) VALUES (12637, 'v12637');
SELECT * FROM "quoted_12638" WHERE col = E'esc\'12638';
# hash comment 12639
-- line 12640: deterministic comment
SELECT `mysql_12641` FROM `tbl_41`;
SELECT `mysql_12642` FROM `tbl_42`;
SELECT * FROM "quoted_12643" WHERE col = E'esc\'12643';
SELECT nested FROM t WHERE id IN (12644, 12645, 12646);
WITH cte_12645 AS (SELECT 12645 AS n) SELECT n FROM cte_12645;
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
# hash comment 12647
SELECT `mysql_12648` FROM `tbl_48`;
SELECT `mysql_12649` FROM `tbl_49`;
SELECT `mysql_12650` FROM `tbl_0`;
SELECT * FROM "quoted_12651" WHERE col = E'esc\'12651';
BEGIN; SELECT 12652; COMMIT;
BEGIN; SELECT 12653; COMMIT;
-- line 12654: deterministic comment
DELETE FROM bench_t_15 WHERE id = 15;
SELECT nested FROM t WHERE id IN (12656, 12657, 12658);
SELECT nested FROM t WHERE id IN (12657, 12658, 12659);
SELECT * FROM "quoted_12658" WHERE col = E'esc\'12658';
/* block header 12659 */
/* block header 12660 */
$dz$ dollar body 12661 ; semicolon inside $dz$
DELETE FROM bench_t_22 WHERE id = 6;
WITH cte_12663 AS (SELECT 12663 AS n) SELECT n FROM cte_12663;
$dz$ dollar body 12664 ; semicolon inside $dz$
SELECT `mysql_12665` FROM `tbl_15`;
INSERT INTO bench_t_122 (id, payload) VALUES (12666, 'v12666');
SELECT `mysql_12667` FROM `tbl_17`;
BEGIN; SELECT 12668; COMMIT;
-- line 12669: deterministic comment
SELECT * FROM "quoted_12670" WHERE col = E'esc\'12670';
$dz$ dollar body 12671 ; semicolon inside $dz$
BEGIN; SELECT 12672; COMMIT;
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
$dz$ dollar body 12674 ; semicolon inside $dz$
# hash comment 12675
SELECT [bracket_12676] FROM [dbo].[tbl_36];
$dz$ dollar body 12677 ; semicolon inside $dz$
SELECT `mysql_12678` FROM `tbl_28`;
UPDATE bench_t_7 SET payload = 12679 WHERE id = 7;
SELECT [bracket_12680] FROM [dbo].[tbl_0];
# hash comment 12681
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
$dz$ dollar body 12683 ; semicolon inside $dz$
-- line 12684: deterministic comment
SELECT nested FROM t WHERE id IN (12685, 12686, 12687);
/* block header 12686 */
DELETE FROM bench_t_15 WHERE id = 15;
# hash comment 12688
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
WITH cte_12690 AS (SELECT 12690 AS n) SELECT n FROM cte_12690;
SELECT [bracket_12691] FROM [dbo].[tbl_11];
BEGIN; SELECT 12692; COMMIT;
DELETE FROM bench_t_21 WHERE id = 5;
SELECT [bracket_12694] FROM [dbo].[tbl_14];
SELECT [bracket_12695] FROM [dbo].[tbl_15];
UPDATE bench_t_24 SET payload = 12696 WHERE id = 24;
WITH cte_12697 AS (SELECT 12697 AS n) SELECT n FROM cte_12697;
-- line 12698: deterministic comment
SELECT nested FROM t WHERE id IN (12699, 12700, 12701);
SELECT nested FROM t WHERE id IN (12700, 12701, 12702);
# hash comment 12701
WITH cte_12702 AS (SELECT 12702 AS n) SELECT n FROM cte_12702;
/* block header 12703 */
$dz$ dollar body 12704 ; semicolon inside $dz$
INSERT INTO bench_t_33 (id, payload) VALUES (12705, 'O''Brien');
SELECT 12706 AS id, 'row_12706' AS label;
$dz$ dollar body 12707 ; semicolon inside $dz$
BEGIN; SELECT 12708; COMMIT;
SELECT * FROM "quoted_12709" WHERE col = E'esc\'12709';
SELECT [bracket_12710] FROM [dbo].[tbl_30];
SELECT [bracket_12711] FROM [dbo].[tbl_31];
WITH cte_12712 AS (SELECT 12712 AS n) SELECT n FROM cte_12712;
# hash comment 12713
DELETE FROM bench_t_10 WHERE id = 10;
# hash comment 12715
$dz$ dollar body 12716 ; semicolon inside $dz$
SELECT 12717 AS id, 'row_12717' AS label;
/* block header 12718 */
/* block header 12719 */
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
WITH cte_12721 AS (SELECT 12721 AS n) SELECT n FROM cte_12721;
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
BEGIN; SELECT 12723; COMMIT;
DELETE FROM bench_t_20 WHERE id = 4;
WITH cte_12725 AS (SELECT 12725 AS n) SELECT n FROM cte_12725;
SELECT `mysql_12726` FROM `tbl_26`;
SELECT [bracket_12727] FROM [dbo].[tbl_7];
-- line 12728: deterministic comment
SELECT * FROM "quoted_12729" WHERE col = E'esc\'12729';
-- line 12730: deterministic comment
SELECT nested FROM t WHERE id IN (12731, 12732, 12733);
SELECT * FROM "quoted_12732" WHERE col = E'esc\'12732';
SELECT [bracket_12733] FROM [dbo].[tbl_13];
SELECT * FROM "quoted_12734" WHERE col = E'esc\'12734';
/* block header 12735 */
SELECT `mysql_12736` FROM `tbl_36`;
BEGIN; SELECT 12737; COMMIT;
$dz$ dollar body 12738 ; semicolon inside $dz$
SELECT 12739 AS id, 'row_12739' AS label;
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT [bracket_12741] FROM [dbo].[tbl_21];
INSERT INTO bench_t_70 (id, payload) VALUES (12742, 'v12742');
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
WITH cte_12744 AS (SELECT 12744 AS n) SELECT n FROM cte_12744;
-- line 12745: deterministic comment
SELECT nested FROM t WHERE id IN (12746, 12747, 12748);
BEGIN; SELECT 12747; COMMIT;
# hash comment 12748
SELECT [bracket_12749] FROM [dbo].[tbl_29];
/*
 * section 51
 * checksum d855
 */
SELECT [bracket_12750] FROM [dbo].[tbl_30];
/* block header 12755 */
SELECT [bracket_12756] FROM [dbo].[tbl_36];
SELECT 12757 AS id, 'row_12757' AS label;
SELECT nested FROM t WHERE id IN (12758, 12759, 12760);
BEGIN; SELECT 12759; COMMIT;
SELECT * FROM "quoted_12760" WHERE col = E'esc\'12760';
SELECT `mysql_12761` FROM `tbl_11`;
SELECT nested FROM t WHERE id IN (12762, 12763, 12764);
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
INSERT INTO bench_t_92 (id, payload) VALUES (12764, 'v12764');
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT 12766 AS id, 'row_12766' AS label;
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
$dz$ dollar body 12768 ; semicolon inside $dz$
DELETE FROM bench_t_1 WHERE id = 1;
-- line 12770: deterministic comment
UPDATE bench_t_35 SET payload = 12771 WHERE id = 3;
-- line 12772: deterministic comment
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
DELETE FROM bench_t_6 WHERE id = 6;
# hash comment 12775
# hash comment 12776
-- line 12777: deterministic comment
UPDATE bench_t_42 SET payload = 12778 WHERE id = 10;
UPDATE bench_t_43 SET payload = 12779 WHERE id = 11;
SELECT * FROM "quoted_12780" WHERE col = E'esc\'12780';
/* block header 12781 */
SELECT [bracket_12782] FROM [dbo].[tbl_22];
SELECT [bracket_12783] FROM [dbo].[tbl_23];
WITH cte_12784 AS (SELECT 12784 AS n) SELECT n FROM cte_12784;
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
-- line 12786: deterministic comment
SELECT [bracket_12787] FROM [dbo].[tbl_27];
SELECT 12788 AS id, 'row_12788' AS label;
# hash comment 12789
WITH cte_12790 AS (SELECT 12790 AS n) SELECT n FROM cte_12790;
WITH cte_12791 AS (SELECT 12791 AS n) SELECT n FROM cte_12791;
$dz$ dollar body 12792 ; semicolon inside $dz$
$dz$ dollar body 12793 ; semicolon inside $dz$
WITH cte_12794 AS (SELECT 12794 AS n) SELECT n FROM cte_12794;
/* block header 12795 */
BEGIN; SELECT 12796; COMMIT;
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT `mysql_12798` FROM `tbl_48`;
-- line 12799: deterministic comment
/* block header 12800 */
WITH cte_12801 AS (SELECT 12801 AS n) SELECT n FROM cte_12801;
SELECT nested FROM t WHERE id IN (12802, 12803, 12804);
SELECT nested FROM t WHERE id IN (12803, 12804, 12805);
/* block header 12804 */
# hash comment 12805
WITH cte_12806 AS (SELECT 12806 AS n) SELECT n FROM cte_12806;
WITH cte_12807 AS (SELECT 12807 AS n) SELECT n FROM cte_12807;
# hash comment 12808
INSERT INTO bench_t_9 (id, payload) VALUES (12809, 'v12809');
# hash comment 12810
SELECT [bracket_12811] FROM [dbo].[tbl_11];
SELECT * FROM "quoted_12812" WHERE col = E'esc\'12812';
SELECT 12813 AS id, 'row_12813' AS label;
$dz$ dollar body 12814 ; semicolon inside $dz$
$dz$ dollar body 12815 ; semicolon inside $dz$
SELECT [bracket_12816] FROM [dbo].[tbl_16];
DELETE FROM bench_t_17 WHERE id = 1;
# hash comment 12818
$dz$ dollar body 12819 ; semicolon inside $dz$
$dz$ dollar body 12820 ; semicolon inside $dz$
SELECT nested FROM t WHERE id IN (12821, 12822, 12823);
SELECT nested FROM t WHERE id IN (12822, 12823, 12824);
# hash comment 12823
UPDATE bench_t_24 SET payload = 12824 WHERE id = 24;
-- line 12825: deterministic comment
SELECT * FROM "quoted_12826" WHERE col = E'esc\'12826';
SELECT `mysql_12827` FROM `tbl_27`;
BEGIN; SELECT 12828; COMMIT;
BEGIN; SELECT 12829; COMMIT;
SELECT * FROM "quoted_12830" WHERE col = E'esc\'12830';
UPDATE bench_t_31 SET payload = 12831 WHERE id = 31;
$dz$ dollar body 12832 ; semicolon inside $dz$
SELECT 12833 AS id, 'row_12833' AS label;
SELECT [bracket_12834] FROM [dbo].[tbl_34];
DELETE FROM bench_t_3 WHERE id = 3;
-- line 12836: deterministic comment
# hash comment 12837
/* block header 12838 */
SELECT 12839 AS id, 'row_12839' AS label;
SELECT * FROM "quoted_12840" WHERE col = E'esc\'12840';
SELECT * FROM "quoted_12841" WHERE col = E'esc\'12841';
SELECT 12842 AS id, 'row_12842' AS label;
SELECT * FROM "quoted_12843" WHERE col = E'esc\'12843';
$dz$ dollar body 12844 ; semicolon inside $dz$
/* block header 12845 */
SELECT `mysql_12846` FROM `tbl_46`;
WITH cte_12847 AS (SELECT 12847 AS n) SELECT n FROM cte_12847;
# hash comment 12848
$dz$ dollar body 12849 ; semicolon inside $dz$
SELECT nested FROM t WHERE id IN (12850, 12851, 12852);
INSERT INTO bench_t_51 (id, payload) VALUES (12851, 'v12851');
SELECT `mysql_12852` FROM `tbl_2`;
SELECT 12853 AS id, 'row_12853' AS label;
WITH cte_12854 AS (SELECT 12854 AS n) SELECT n FROM cte_12854;
/* block header 12855 */
SELECT * FROM "quoted_12856" WHERE col = E'esc\'12856';
-- line 12857: deterministic comment
# hash comment 12858
WITH cte_12859 AS (SELECT 12859 AS n) SELECT n FROM cte_12859;
WITH cte_12860 AS (SELECT 12860 AS n) SELECT n FROM cte_12860;
WITH cte_12861 AS (SELECT 12861 AS n) SELECT n FROM cte_12861;
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
UPDATE bench_t_63 SET payload = 12863 WHERE id = 31;
UPDATE bench_t_0 SET payload = 12864 WHERE id = 0;
BEGIN; SELECT 12865; COMMIT;
SELECT [bracket_12866] FROM [dbo].[tbl_26];
WITH cte_12867 AS (SELECT 12867 AS n) SELECT n FROM cte_12867;
-- line 12868: deterministic comment
WITH cte_12869 AS (SELECT 12869 AS n) SELECT n FROM cte_12869;
WITH cte_12870 AS (SELECT 12870 AS n) SELECT n FROM cte_12870;
SELECT 12871 AS id, 'row_12871' AS label;
SELECT `mysql_12872` FROM `tbl_22`;
SELECT nested FROM t WHERE id IN (12873, 12874, 12875);
SELECT 12874 AS id, 'row_12874' AS label;
UPDATE bench_t_11 SET payload = 12875 WHERE id = 11;
BEGIN; SELECT 12876; COMMIT;
SELECT nested FROM t WHERE id IN (12877, 12878, 12879);
-- line 12878: deterministic comment
DELETE FROM bench_t_15 WHERE id = 15;
WITH cte_12880 AS (SELECT 12880 AS n) SELECT n FROM cte_12880;
SELECT * FROM "quoted_12881" WHERE col = E'esc\'12881';
INSERT INTO bench_t_82 (id, payload) VALUES (12882, 'v12882');
/* block header 12883 */
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT * FROM "quoted_12886" WHERE col = E'esc\'12886';
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
UPDATE bench_t_24 SET payload = 12888 WHERE id = 24;
WITH cte_12889 AS (SELECT 12889 AS n) SELECT n FROM cte_12889;
$dz$ dollar body 12890 ; semicolon inside $dz$
UPDATE bench_t_27 SET payload = 12891 WHERE id = 27;
SELECT 12892 AS id, 'row_12892' AS label;
DELETE FROM bench_t_29 WHERE id = 13;
SELECT nested FROM t WHERE id IN (12894, 12895, 12896);
SELECT nested FROM t WHERE id IN (12895, 12896, 12897);
SELECT nested FROM t WHERE id IN (12896, 12897, 12898);
SELECT nested FROM t WHERE id IN (12897, 12898, 12899);
SELECT 12898 AS id, 'row_12898' AS label;
UPDATE bench_t_35 SET payload = 12899 WHERE id = 3;
SELECT * FROM "quoted_12900" WHERE col = E'esc\'12900';
BEGIN; SELECT 12901; COMMIT;
SELECT `mysql_12902` FROM `tbl_2`;
SELECT * FROM "quoted_12903" WHERE col = E'esc\'12903';
-- line 12904: deterministic comment
INSERT INTO bench_t_105 (id, payload) VALUES (12905, 'v12905');
SELECT 12906 AS id, 'row_12906' AS label;
DELETE FROM bench_t_11 WHERE id = 11;
# hash comment 12908
SELECT 12909 AS id, 'row_12909' AS label;
$dz$ dollar body 12910 ; semicolon inside $dz$
-- line 12911: deterministic comment
BEGIN; SELECT 12912; COMMIT;
SELECT nested FROM t WHERE id IN (12913, 12914, 12915);
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
WITH cte_12915 AS (SELECT 12915 AS n) SELECT n FROM cte_12915;
SELECT `mysql_12916` FROM `tbl_16`;
/* block header 12917 */
UPDATE bench_t_54 SET payload = 12918 WHERE id = 22;
/* block header 12919 */
SELECT 12920 AS id, 'row_12920' AS label;
UPDATE bench_t_57 SET payload = 12921 WHERE id = 25;
-- line 12922: deterministic comment
SELECT `mysql_12923` FROM `tbl_23`;
DELETE FROM bench_t_28 WHERE id = 12;
$dz$ dollar body 12925 ; semicolon inside $dz$
$dz$ dollar body 12926 ; semicolon inside $dz$
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
DELETE FROM bench_t_1 WHERE id = 1;
SELECT 12930 AS id, 'row_12930' AS label;
SELECT `mysql_12931` FROM `tbl_31`;
SELECT * FROM "quoted_12932" WHERE col = E'esc\'12932';
-- line 12933: deterministic comment
DELETE FROM bench_t_6 WHERE id = 6;
SELECT nested FROM t WHERE id IN (12935, 12936, 12937);
-- line 12936: deterministic comment
SELECT nested FROM t WHERE id IN (12937, 12938, 12939);
SELECT `mysql_12938` FROM `tbl_38`;
DELETE FROM bench_t_11 WHERE id = 11;
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
BEGIN; SELECT 12941; COMMIT;
SELECT nested FROM t WHERE id IN (12942, 12943, 12944);
/* block header 12943 */
BEGIN; SELECT 12944; COMMIT;
SELECT nested FROM t WHERE id IN (12945, 12946, 12947);
BEGIN; SELECT 12946; COMMIT;
BEGIN; SELECT 12947; COMMIT;
DELETE FROM bench_t_20 WHERE id = 4;
DELETE FROM bench_t_21 WHERE id = 5;
SELECT `mysql_12950` FROM `tbl_0`;
UPDATE bench_t_23 SET payload = 12951 WHERE id = 23;
/* block header 12952 */
-- line 12953: deterministic comment
/* block header 12954 */
SELECT 12955 AS id, 'row_12955' AS label;
-- line 12956: deterministic comment
$dz$ dollar body 12957 ; semicolon inside $dz$
/* block header 12958 */
DELETE FROM bench_t_31 WHERE id = 15;
# hash comment 12960
-- line 12961: deterministic comment
/* block header 12962 */
BEGIN; SELECT 12963; COMMIT;
BEGIN; SELECT 12964; COMMIT;
-- line 12965: deterministic comment
DELETE FROM bench_t_6 WHERE id = 6;
SELECT nested FROM t WHERE id IN (12967, 12968, 12969);
$dz$ dollar body 12968 ; semicolon inside $dz$
DELETE FROM bench_t_9 WHERE id = 9;
# hash comment 12970
WITH cte_12971 AS (SELECT 12971 AS n) SELECT n FROM cte_12971;
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT nested FROM t WHERE id IN (12973, 12974, 12975);
/* block header 12974 */
BEGIN; SELECT 12975; COMMIT;
WITH cte_12976 AS (SELECT 12976 AS n) SELECT n FROM cte_12976;
# hash comment 12977
-- line 12978: deterministic comment
SELECT * FROM "quoted_12979" WHERE col = E'esc\'12979';
-- line 12980: deterministic comment
SELECT 12981 AS id, 'row_12981' AS label;
INSERT INTO bench_t_54 (id, payload) VALUES (12982, 'v12982');
DELETE FROM bench_t_23 WHERE id = 7;
BEGIN; SELECT 12984; COMMIT;
INSERT INTO bench_t_57 (id, payload) VALUES (12985, 'v12985');
SELECT `mysql_12986` FROM `tbl_36`;
$dz$ dollar body 12987 ; semicolon inside $dz$
-- line 12988: deterministic comment
$dz$ dollar body 12989 ; semicolon inside $dz$
INSERT INTO bench_t_62 (id, payload) VALUES (12990, 'v12990');
UPDATE bench_t_63 SET payload = 12991 WHERE id = 31;
SELECT `mysql_12992` FROM `tbl_42`;
UPDATE bench_t_1 SET payload = 12993 WHERE id = 1;
$dz$ dollar body 12994 ; semicolon inside $dz$
INSERT INTO bench_t_67 (id, payload) VALUES (12995, 'v12995');
WITH cte_12996 AS (SELECT 12996 AS n) SELECT n FROM cte_12996;
# hash comment 12997
/* block header 12998 */
/* block header 12999 */
/*
 * section 52
 * checksum 69d3
 */
SELECT 13000 AS id, 'row_13000' AS label;
BEGIN; SELECT 13005; COMMIT;
DELETE FROM bench_t_14 WHERE id = 14;
SELECT 13007 AS id, 'row_13007' AS label;
SELECT `mysql_13008` FROM `tbl_8`;
SELECT 13009 AS id, 'row_13009' AS label;
SELECT `mysql_13010` FROM `tbl_10`;
WITH cte_13011 AS (SELECT 13011 AS n) SELECT n FROM cte_13011;
DELETE FROM bench_t_20 WHERE id = 4;
BEGIN; SELECT 13013; COMMIT;
SELECT `mysql_13014` FROM `tbl_14`;
# hash comment 13015
SELECT nested FROM t WHERE id IN (13016, 13017, 13018);
SELECT [bracket_13017] FROM [dbo].[tbl_17];
SELECT 13018 AS id, 'row_13018' AS label;
SELECT nested FROM t WHERE id IN (13019, 13020, 13021);
/* block header 13020 */
# hash comment 13021
/* block header 13022 */
SELECT * FROM "quoted_13023" WHERE col = E'esc\'13023';
-- line 13024: deterministic comment
SELECT nested FROM t WHERE id IN (13025, 13026, 13027);
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
BEGIN; SELECT 13027; COMMIT;
SELECT nested FROM t WHERE id IN (13028, 13029, 13030);
SELECT `mysql_13029` FROM `tbl_29`;
INSERT INTO bench_t_102 (id, payload) VALUES (13030, 'v13030');
UPDATE bench_t_39 SET payload = 13031 WHERE id = 7;
/* block header 13032 */
# hash comment 13033
# hash comment 13034
SELECT [bracket_13035] FROM [dbo].[tbl_35];
$dz$ dollar body 13036 ; semicolon inside $dz$
BEGIN; SELECT 13037; COMMIT;
/* block header 13038 */
DELETE FROM bench_t_15 WHERE id = 15;
WITH cte_13040 AS (SELECT 13040 AS n) SELECT n FROM cte_13040;
# hash comment 13041
BEGIN; SELECT 13042; COMMIT;
SELECT 13043 AS id, 'row_13043' AS label;
SELECT nested FROM t WHERE id IN (13044, 13045, 13046);
SELECT * FROM "quoted_13045" WHERE col = E'esc\'13045';
SELECT nested FROM t WHERE id IN (13046, 13047, 13048);
/* block header 13047 */
-- line 13048: deterministic comment
SELECT `mysql_13049` FROM `tbl_49`;
SELECT [bracket_13050] FROM [dbo].[tbl_10];
$dz$ dollar body 13051 ; semicolon inside $dz$
-- line 13052: deterministic comment
SELECT nested FROM t WHERE id IN (13053, 13054, 13055);
SELECT * FROM "quoted_13054" WHERE col = E'esc\'13054';
SELECT nested FROM t WHERE id IN (13055, 13056, 13057);
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
# hash comment 13057
SELECT 13058 AS id, 'row_13058' AS label;
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
UPDATE bench_t_4 SET payload = 13060 WHERE id = 4;
$dz$ dollar body 13061 ; semicolon inside $dz$
BEGIN; SELECT 13062; COMMIT;
/* block header 13063 */
DELETE FROM bench_t_8 WHERE id = 8;
SELECT nested FROM t WHERE id IN (13065, 13066, 13067);
SELECT * FROM "quoted_13066" WHERE col = E'esc\'13066';
SELECT `mysql_13067` FROM `tbl_17`;
$dz$ dollar body 13068 ; semicolon inside $dz$
-- line 13069: deterministic comment
UPDATE bench_t_14 SET payload = 13070 WHERE id = 14;
SELECT [bracket_13071] FROM [dbo].[tbl_31];
$dz$ dollar body 13072 ; semicolon inside $dz$
# hash comment 13073
SELECT 13074 AS id, 'row_13074' AS label;
SELECT `mysql_13075` FROM `tbl_25`;
DELETE FROM bench_t_20 WHERE id = 4;
$dz$ dollar body 13077 ; semicolon inside $dz$
SELECT `mysql_13078` FROM `tbl_28`;
-- line 13079: deterministic comment
SELECT [bracket_13080] FROM [dbo].[tbl_0];
SELECT 13081 AS id, 'row_13081' AS label;
/* block header 13082 */
WITH cte_13083 AS (SELECT 13083 AS n) SELECT n FROM cte_13083;
/* block header 13084 */
# hash comment 13085
BEGIN; SELECT 13086; COMMIT;
UPDATE bench_t_31 SET payload = 13087 WHERE id = 31;
# hash comment 13088
SELECT 13089 AS id, 'row_13089' AS label;
UPDATE bench_t_34 SET payload = 13090 WHERE id = 2;
/* block header 13091 */
DELETE FROM bench_t_4 WHERE id = 4;
UPDATE bench_t_37 SET payload = 13093 WHERE id = 5;
SELECT [bracket_13094] FROM [dbo].[tbl_14];
SELECT [bracket_13095] FROM [dbo].[tbl_15];
WITH cte_13096 AS (SELECT 13096 AS n) SELECT n FROM cte_13096;
UPDATE bench_t_41 SET payload = 13097 WHERE id = 9;
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
UPDATE bench_t_43 SET payload = 13099 WHERE id = 11;
SELECT `mysql_13100` FROM `tbl_0`;
SELECT `mysql_13101` FROM `tbl_1`;
# hash comment 13102
SELECT nested FROM t WHERE id IN (13103, 13104, 13105);
# hash comment 13104
UPDATE bench_t_49 SET payload = 13105 WHERE id = 17;
SELECT nested FROM t WHERE id IN (13106, 13107, 13108);
INSERT INTO bench_t_51 (id, payload) VALUES (13107, 'v13107');
-- line 13108: deterministic comment
WITH cte_13109 AS (SELECT 13109 AS n) SELECT n FROM cte_13109;
SELECT 13110 AS id, 'row_13110' AS label;
DELETE FROM bench_t_23 WHERE id = 7;
DELETE FROM bench_t_24 WHERE id = 8;
INSERT INTO bench_t_57 (id, payload) VALUES (13113, 'v13113');
# hash comment 13114
SELECT nested FROM t WHERE id IN (13115, 13116, 13117);
SELECT nested FROM t WHERE id IN (13116, 13117, 13118);
SELECT nested FROM t WHERE id IN (13117, 13118, 13119);
$dz$ dollar body 13118 ; semicolon inside $dz$
$dz$ dollar body 13119 ; semicolon inside $dz$
SELECT [bracket_13120] FROM [dbo].[tbl_0];
/* block header 13121 */
SELECT `mysql_13122` FROM `tbl_22`;
-- line 13123: deterministic comment
DELETE FROM bench_t_4 WHERE id = 4;
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
/* block header 13126 */
SELECT [bracket_13127] FROM [dbo].[tbl_7];
SELECT 13128 AS id, 'row_13128' AS label;
SELECT [bracket_13129] FROM [dbo].[tbl_9];
INSERT INTO bench_t_74 (id, payload) VALUES (13130, 'v13130');
SELECT * FROM "quoted_13131" WHERE col = E'esc\'13131';
SELECT nested FROM t WHERE id IN (13132, 13133, 13134);
SELECT 13133 AS id, 'row_13133' AS label;
DELETE FROM bench_t_14 WHERE id = 14;
SELECT 13135 AS id, 'row_13135' AS label;
-- line 13136: deterministic comment
SELECT `mysql_13137` FROM `tbl_37`;
SELECT nested FROM t WHERE id IN (13138, 13139, 13140);
SELECT 13139 AS id, 'row_13139' AS label;
# hash comment 13140
UPDATE bench_t_21 SET payload = 13141 WHERE id = 21;
# hash comment 13142
# hash comment 13143
-- line 13144: deterministic comment
SELECT nested FROM t WHERE id IN (13145, 13146, 13147);
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
-- line 13147: deterministic comment
SELECT `mysql_13148` FROM `tbl_48`;
SELECT [bracket_13149] FROM [dbo].[tbl_29];
INSERT INTO bench_t_94 (id, payload) VALUES (13150, 'v13150');
$dz$ dollar body 13151 ; semicolon inside $dz$
SELECT [bracket_13152] FROM [dbo].[tbl_32];
BEGIN; SELECT 13153; COMMIT;
SELECT `mysql_13154` FROM `tbl_4`;
$dz$ dollar body 13155 ; semicolon inside $dz$
UPDATE bench_t_36 SET payload = 13156 WHERE id = 4;
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
UPDATE bench_t_38 SET payload = 13158 WHERE id = 6;
SELECT 13159 AS id, 'row_13159' AS label;
UPDATE bench_t_40 SET payload = 13160 WHERE id = 8;
SELECT * FROM "quoted_13161" WHERE col = E'esc\'13161';
SELECT `mysql_13162` FROM `tbl_12`;
SELECT [bracket_13163] FROM [dbo].[tbl_3];
DELETE FROM bench_t_12 WHERE id = 12;
$dz$ dollar body 13165 ; semicolon inside $dz$
SELECT 13166 AS id, 'row_13166' AS label;
SELECT nested FROM t WHERE id IN (13167, 13168, 13169);
SELECT 13168 AS id, 'row_13168' AS label;
SELECT [bracket_13169] FROM [dbo].[tbl_9];
# hash comment 13170
SELECT [bracket_13171] FROM [dbo].[tbl_11];
/* block header 13172 */
BEGIN; SELECT 13173; COMMIT;
BEGIN; SELECT 13174; COMMIT;
/* block header 13175 */
SELECT nested FROM t WHERE id IN (13176, 13177, 13178);
BEGIN; SELECT 13177; COMMIT;
SELECT [bracket_13178] FROM [dbo].[tbl_18];
SELECT nested FROM t WHERE id IN (13179, 13180, 13181);
SELECT * FROM "quoted_13180" WHERE col = E'esc\'13180';
SELECT `mysql_13181` FROM `tbl_31`;
-- line 13182: deterministic comment
BEGIN; SELECT 13183; COMMIT;
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
BEGIN; SELECT 13185; COMMIT;
/* block header 13186 */
-- line 13187: deterministic comment
SELECT `mysql_13188` FROM `tbl_38`;
BEGIN; SELECT 13189; COMMIT;
SELECT `mysql_13190` FROM `tbl_40`;
SELECT nested FROM t WHERE id IN (13191, 13192, 13193);
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT * FROM "quoted_13193" WHERE col = E'esc\'13193';
$dz$ dollar body 13194 ; semicolon inside $dz$
UPDATE bench_t_11 SET payload = 13195 WHERE id = 11;
SELECT `mysql_13196` FROM `tbl_46`;
SELECT 13197 AS id, 'row_13197' AS label;
SELECT nested FROM t WHERE id IN (13198, 13199, 13200);
WITH cte_13199 AS (SELECT 13199 AS n) SELECT n FROM cte_13199;
DELETE FROM bench_t_16 WHERE id = 0;
# hash comment 13201
SELECT 13202 AS id, 'row_13202' AS label;
SELECT [bracket_13203] FROM [dbo].[tbl_3];
BEGIN; SELECT 13204; COMMIT;
# hash comment 13205
# hash comment 13206
$dz$ dollar body 13207 ; semicolon inside $dz$
-- line 13208: deterministic comment
/* block header 13209 */
UPDATE bench_t_26 SET payload = 13210 WHERE id = 26;
SELECT [bracket_13211] FROM [dbo].[tbl_11];
BEGIN; SELECT 13212; COMMIT;
WITH cte_13213 AS (SELECT 13213 AS n) SELECT n FROM cte_13213;
BEGIN; SELECT 13214; COMMIT;
SELECT nested FROM t WHERE id IN (13215, 13216, 13217);
/* block header 13216 */
BEGIN; SELECT 13217; COMMIT;
SELECT 13218 AS id, 'row_13218' AS label;
WITH cte_13219 AS (SELECT 13219 AS n) SELECT n FROM cte_13219;
BEGIN; SELECT 13220; COMMIT;
SELECT [bracket_13221] FROM [dbo].[tbl_21];
DELETE FROM bench_t_6 WHERE id = 6;
SELECT `mysql_13223` FROM `tbl_23`;
# hash comment 13224
SELECT 13225 AS id, 'row_13225' AS label;
-- line 13226: deterministic comment
SELECT * FROM "quoted_13227" WHERE col = E'esc\'13227';
BEGIN; SELECT 13228; COMMIT;
# hash comment 13229
DELETE FROM bench_t_14 WHERE id = 14;
-- line 13231: deterministic comment
UPDATE bench_t_48 SET payload = 13232 WHERE id = 16;
SELECT 13233 AS id, 'row_13233' AS label;
SELECT 13234 AS id, 'row_13234' AS label;
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
$dz$ dollar body 13236 ; semicolon inside $dz$
WITH cte_13237 AS (SELECT 13237 AS n) SELECT n FROM cte_13237;
SELECT nested FROM t WHERE id IN (13238, 13239, 13240);
SELECT `mysql_13239` FROM `tbl_39`;
SELECT * FROM "quoted_13240" WHERE col = E'esc\'13240';
UPDATE bench_t_57 SET payload = 13241 WHERE id = 25;
DELETE FROM bench_t_26 WHERE id = 10;
BEGIN; SELECT 13243; COMMIT;
SELECT `mysql_13244` FROM `tbl_44`;
SELECT 13245 AS id, 'row_13245' AS label;
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
BEGIN; SELECT 13247; COMMIT;
UPDATE bench_t_0 SET payload = 13248 WHERE id = 0;
DELETE FROM bench_t_1 WHERE id = 1;
/*
 * section 53
 * checksum 2342
 */
SELECT nested FROM t WHERE id IN (13250, 13251, 13252);
SELECT 13255 AS id, 'row_13255' AS label;
SELECT * FROM "quoted_13256" WHERE col = E'esc\'13256';
# hash comment 13257
INSERT INTO bench_t_74 (id, payload) VALUES (13258, 'v13258');
SELECT * FROM "quoted_13259" WHERE col = E'esc\'13259';
-- line 13260: deterministic comment
$dz$ dollar body 13261 ; semicolon inside $dz$
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT * FROM "quoted_13263" WHERE col = E'esc\'13263';
/* block header 13264 */
WITH cte_13265 AS (SELECT 13265 AS n) SELECT n FROM cte_13265;
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
WITH cte_13267 AS (SELECT 13267 AS n) SELECT n FROM cte_13267;
$dz$ dollar body 13268 ; semicolon inside $dz$
SELECT 13269 AS id, 'row_13269' AS label;
INSERT INTO bench_t_86 (id, payload) VALUES (13270, 'v13270');
/* block header 13271 */
/* block header 13272 */
# hash comment 13273
# hash comment 13274
WITH cte_13275 AS (SELECT 13275 AS n) SELECT n FROM cte_13275;
SELECT 13276 AS id, 'row_13276' AS label;
SELECT [bracket_13277] FROM [dbo].[tbl_37];
UPDATE bench_t_30 SET payload = 13278 WHERE id = 30;
SELECT * FROM "quoted_13279" WHERE col = E'esc\'13279';
SELECT 13280 AS id, 'row_13280' AS label;
SELECT `mysql_13281` FROM `tbl_31`;
# hash comment 13282
WITH cte_13283 AS (SELECT 13283 AS n) SELECT n FROM cte_13283;
SELECT nested FROM t WHERE id IN (13284, 13285, 13286);
SELECT 13285 AS id, 'row_13285' AS label;
BEGIN; SELECT 13286; COMMIT;
# hash comment 13287
DELETE FROM bench_t_8 WHERE id = 8;
WITH cte_13289 AS (SELECT 13289 AS n) SELECT n FROM cte_13289;
SELECT * FROM "quoted_13290" WHERE col = E'esc\'13290';
SELECT * FROM "quoted_13291" WHERE col = E'esc\'13291';
SELECT `mysql_13292` FROM `tbl_42`;
WITH cte_13293 AS (SELECT 13293 AS n) SELECT n FROM cte_13293;
/* block header 13294 */
SELECT * FROM "quoted_13295" WHERE col = E'esc\'13295';
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT [bracket_13297] FROM [dbo].[tbl_17];
INSERT INTO bench_t_114 (id, payload) VALUES (13298, 'v13298');
SELECT [bracket_13299] FROM [dbo].[tbl_19];
INSERT INTO bench_t_116 (id, payload) VALUES (13300, 'v13300');
INSERT INTO bench_t_117 (id, payload) VALUES (13301, 'v13301');
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
-- line 13303: deterministic comment
SELECT `mysql_13304` FROM `tbl_4`;
SELECT `mysql_13305` FROM `tbl_5`;
SELECT `mysql_13306` FROM `tbl_6`;
INSERT INTO bench_t_123 (id, payload) VALUES (13307, 'v13307');
INSERT INTO bench_t_124 (id, payload) VALUES (13308, 'v13308');
SELECT 13309 AS id, 'row_13309' AS label;
$dz$ dollar body 13310 ; semicolon inside $dz$
/* block header 13311 */
/* block header 13312 */
/* block header 13313 */
# hash comment 13314
SELECT * FROM "quoted_13315" WHERE col = E'esc\'13315';
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
BEGIN; SELECT 13317; COMMIT;
SELECT `mysql_13318` FROM `tbl_18`;
WITH cte_13319 AS (SELECT 13319 AS n) SELECT n FROM cte_13319;
$dz$ dollar body 13320 ; semicolon inside $dz$
INSERT INTO bench_t_9 (id, payload) VALUES (13321, 'O''Brien');
SELECT 13322 AS id, 'row_13322' AS label;
UPDATE bench_t_11 SET payload = 13323 WHERE id = 11;
-- line 13324: deterministic comment
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
WITH cte_13326 AS (SELECT 13326 AS n) SELECT n FROM cte_13326;
WITH cte_13327 AS (SELECT 13327 AS n) SELECT n FROM cte_13327;
SELECT [bracket_13328] FROM [dbo].[tbl_8];
SELECT [bracket_13329] FROM [dbo].[tbl_9];
SELECT nested FROM t WHERE id IN (13330, 13331, 13332);
UPDATE bench_t_19 SET payload = 13331 WHERE id = 19;
/* block header 13332 */
$dz$ dollar body 13333 ; semicolon inside $dz$
$dz$ dollar body 13334 ; semicolon inside $dz$
/* block header 13335 */
SELECT [bracket_13336] FROM [dbo].[tbl_16];
WITH cte_13337 AS (SELECT 13337 AS n) SELECT n FROM cte_13337;
INSERT INTO bench_t_26 (id, payload) VALUES (13338, 'v13338');
UPDATE bench_t_27 SET payload = 13339 WHERE id = 27;
UPDATE bench_t_28 SET payload = 13340 WHERE id = 28;
SELECT `mysql_13341` FROM `tbl_41`;
DELETE FROM bench_t_30 WHERE id = 14;
/* block header 13343 */
-- line 13344: deterministic comment
INSERT INTO bench_t_33 (id, payload) VALUES (13345, 'v13345');
UPDATE bench_t_34 SET payload = 13346 WHERE id = 2;
SELECT [bracket_13347] FROM [dbo].[tbl_27];
WITH cte_13348 AS (SELECT 13348 AS n) SELECT n FROM cte_13348;
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
INSERT INTO bench_t_38 (id, payload) VALUES (13350, 'v13350');
WITH cte_13351 AS (SELECT 13351 AS n) SELECT n FROM cte_13351;
DELETE FROM bench_t_8 WHERE id = 8;
$dz$ dollar body 13353 ; semicolon inside $dz$
SELECT 13354 AS id, 'row_13354' AS label;
SELECT * FROM "quoted_13355" WHERE col = E'esc\'13355';
BEGIN; SELECT 13356; COMMIT;
BEGIN; SELECT 13357; COMMIT;
UPDATE bench_t_46 SET payload = 13358 WHERE id = 14;
BEGIN; SELECT 13359; COMMIT;
SELECT `mysql_13360` FROM `tbl_10`;
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT * FROM "quoted_13363" WHERE col = E'esc\'13363';
BEGIN; SELECT 13364; COMMIT;
SELECT 13365 AS id, 'row_13365' AS label;
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT 13367 AS id, 'row_13367' AS label;
/* block header 13368 */
SELECT [bracket_13369] FROM [dbo].[tbl_9];
SELECT [bracket_13370] FROM [dbo].[tbl_10];
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
BEGIN; SELECT 13372; COMMIT;
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
# hash comment 13375
SELECT `mysql_13376` FROM `tbl_26`;
# hash comment 13377
/* block header 13378 */
SELECT [bracket_13379] FROM [dbo].[tbl_19];
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
WITH cte_13381 AS (SELECT 13381 AS n) SELECT n FROM cte_13381;
$dz$ dollar body 13382 ; semicolon inside $dz$
UPDATE bench_t_7 SET payload = 13383 WHERE id = 7;
INSERT INTO bench_t_72 (id, payload) VALUES (13384, 'v13384');
INSERT INTO bench_t_73 (id, payload) VALUES (13385, 'v13385');
/* block header 13386 */
SELECT nested FROM t WHERE id IN (13387, 13388, 13389);
SELECT [bracket_13388] FROM [dbo].[tbl_28];
WITH cte_13389 AS (SELECT 13389 AS n) SELECT n FROM cte_13389;
INSERT INTO bench_t_78 (id, payload) VALUES (13390, 'v13390');
# hash comment 13391
SELECT nested FROM t WHERE id IN (13392, 13393, 13394);
# hash comment 13393
BEGIN; SELECT 13394; COMMIT;
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
$dz$ dollar body 13397 ; semicolon inside $dz$
SELECT [bracket_13398] FROM [dbo].[tbl_38];
# hash comment 13399
SELECT [bracket_13400] FROM [dbo].[tbl_0];
UPDATE bench_t_25 SET payload = 13401 WHERE id = 25;
# hash comment 13402
DELETE FROM bench_t_27 WHERE id = 11;
WITH cte_13404 AS (SELECT 13404 AS n) SELECT n FROM cte_13404;
UPDATE bench_t_29 SET payload = 13405 WHERE id = 29;
UPDATE bench_t_30 SET payload = 13406 WHERE id = 30;
SELECT [bracket_13407] FROM [dbo].[tbl_7];
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT nested FROM t WHERE id IN (13409, 13410, 13411);
SELECT * FROM "quoted_13410" WHERE col = E'esc\'13410';
INSERT INTO bench_t_99 (id, payload) VALUES (13411, 'v13411');
SELECT [bracket_13412] FROM [dbo].[tbl_12];
WITH cte_13413 AS (SELECT 13413 AS n) SELECT n FROM cte_13413;
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT `mysql_13415` FROM `tbl_15`;
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
$dz$ dollar body 13417 ; semicolon inside $dz$
# hash comment 13418
SELECT `mysql_13419` FROM `tbl_19`;
SELECT 13420 AS id, 'row_13420' AS label;
WITH cte_13421 AS (SELECT 13421 AS n) SELECT n FROM cte_13421;
SELECT [bracket_13422] FROM [dbo].[tbl_22];
SELECT 13423 AS id, 'row_13423' AS label;
/* block header 13424 */
/* block header 13425 */
SELECT 13426 AS id, 'row_13426' AS label;
SELECT `mysql_13427` FROM `tbl_27`;
/* block header 13428 */
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
/* block header 13430 */
SELECT nested FROM t WHERE id IN (13431, 13432, 13433);
# hash comment 13432
INSERT INTO bench_t_121 (id, payload) VALUES (13433, 'v13433');
INSERT INTO bench_t_122 (id, payload) VALUES (13434, 'v13434');
$dz$ dollar body 13435 ; semicolon inside $dz$
BEGIN; SELECT 13436; COMMIT;
SELECT nested FROM t WHERE id IN (13437, 13438, 13439);
SELECT `mysql_13438` FROM `tbl_38`;
SELECT nested FROM t WHERE id IN (13439, 13440, 13441);
SELECT 13440 AS id, 'row_13440' AS label;
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
WITH cte_13442 AS (SELECT 13442 AS n) SELECT n FROM cte_13442;
SELECT `mysql_13443` FROM `tbl_43`;
/* block header 13444 */
WITH cte_13445 AS (SELECT 13445 AS n) SELECT n FROM cte_13445;
-- line 13446: deterministic comment
SELECT nested FROM t WHERE id IN (13447, 13448, 13449);
SELECT 13448 AS id, 'row_13448' AS label;
SELECT * FROM "quoted_13449" WHERE col = E'esc\'13449';
DELETE FROM bench_t_10 WHERE id = 10;
SELECT nested FROM t WHERE id IN (13451, 13452, 13453);
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
UPDATE bench_t_13 SET payload = 13453 WHERE id = 13;
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
DELETE FROM bench_t_15 WHERE id = 15;
-- line 13456: deterministic comment
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
BEGIN; SELECT 13458; COMMIT;
# hash comment 13459
/* block header 13460 */
UPDATE bench_t_21 SET payload = 13461 WHERE id = 21;
SELECT 13462 AS id, 'row_13462' AS label;
UPDATE bench_t_23 SET payload = 13463 WHERE id = 23;
# hash comment 13464
# hash comment 13465
SELECT nested FROM t WHERE id IN (13466, 13467, 13468);
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
INSERT INTO bench_t_28 (id, payload) VALUES (13468, 'v13468');
SELECT [bracket_13469] FROM [dbo].[tbl_29];
# hash comment 13470
DELETE FROM bench_t_31 WHERE id = 15;
-- line 13472: deterministic comment
SELECT [bracket_13473] FROM [dbo].[tbl_33];
WITH cte_13474 AS (SELECT 13474 AS n) SELECT n FROM cte_13474;
$dz$ dollar body 13475 ; semicolon inside $dz$
-- line 13476: deterministic comment
SELECT `mysql_13477` FROM `tbl_27`;
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT `mysql_13479` FROM `tbl_29`;
/* block header 13480 */
SELECT `mysql_13481` FROM `tbl_31`;
SELECT 13482 AS id, 'row_13482' AS label;
SELECT `mysql_13483` FROM `tbl_33`;
# hash comment 13484
BEGIN; SELECT 13485; COMMIT;
-- line 13486: deterministic comment
SELECT [bracket_13487] FROM [dbo].[tbl_7];
UPDATE bench_t_48 SET payload = 13488 WHERE id = 16;
DELETE FROM bench_t_17 WHERE id = 1;
SELECT nested FROM t WHERE id IN (13490, 13491, 13492);
SELECT [bracket_13491] FROM [dbo].[tbl_11];
INSERT INTO bench_t_52 (id, payload) VALUES (13492, 'v13492');
SELECT [bracket_13493] FROM [dbo].[tbl_13];
$dz$ dollar body 13494 ; semicolon inside $dz$
WITH cte_13495 AS (SELECT 13495 AS n) SELECT n FROM cte_13495;
BEGIN; SELECT 13496; COMMIT;
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT `mysql_13498` FROM `tbl_48`;
# hash comment 13499
/*
 * section 54
 * checksum 92e3
 */
SELECT nested FROM t WHERE id IN (13500, 13501, 13502);
-- line 13505: deterministic comment
DELETE FROM bench_t_2 WHERE id = 2;
INSERT INTO bench_t_67 (id, payload) VALUES (13507, 'v13507');
BEGIN; SELECT 13508; COMMIT;
SELECT [bracket_13509] FROM [dbo].[tbl_29];
/* block header 13510 */
SELECT 13511 AS id, 'row_13511' AS label;
# hash comment 13512
$dz$ dollar body 13513 ; semicolon inside $dz$
SELECT nested FROM t WHERE id IN (13514, 13515, 13516);
SELECT `mysql_13515` FROM `tbl_15`;
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
BEGIN; SELECT 13517; COMMIT;
SELECT * FROM "quoted_13518" WHERE col = E'esc\'13518';
UPDATE bench_t_15 SET payload = 13519 WHERE id = 15;
WITH cte_13520 AS (SELECT 13520 AS n) SELECT n FROM cte_13520;
BEGIN; SELECT 13521; COMMIT;
UPDATE bench_t_18 SET payload = 13522 WHERE id = 18;
-- line 13523: deterministic comment
/* block header 13524 */
DELETE FROM bench_t_21 WHERE id = 5;
SELECT [bracket_13526] FROM [dbo].[tbl_6];
UPDATE bench_t_23 SET payload = 13527 WHERE id = 23;
SELECT 13528 AS id, 'row_13528' AS label;
SELECT `mysql_13529` FROM `tbl_29`;
SELECT 13530 AS id, 'row_13530' AS label;
SELECT * FROM "quoted_13531" WHERE col = E'esc\'13531';
# hash comment 13532
SELECT nested FROM t WHERE id IN (13533, 13534, 13535);
SELECT `mysql_13534` FROM `tbl_34`;
UPDATE bench_t_31 SET payload = 13535 WHERE id = 31;
SELECT nested FROM t WHERE id IN (13536, 13537, 13538);
# hash comment 13537
SELECT `mysql_13538` FROM `tbl_38`;
WITH cte_13539 AS (SELECT 13539 AS n) SELECT n FROM cte_13539;
DELETE FROM bench_t_4 WHERE id = 4;
WITH cte_13541 AS (SELECT 13541 AS n) SELECT n FROM cte_13541;
SELECT nested FROM t WHERE id IN (13542, 13543, 13544);
# hash comment 13543
UPDATE bench_t_40 SET payload = 13544 WHERE id = 8;
BEGIN; SELECT 13545; COMMIT;
/* block header 13546 */
DELETE FROM bench_t_11 WHERE id = 11;
INSERT INTO bench_t_108 (id, payload) VALUES (13548, 'v13548');
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT [bracket_13550] FROM [dbo].[tbl_30];
UPDATE bench_t_47 SET payload = 13551 WHERE id = 15;
BEGIN; SELECT 13552; COMMIT;
BEGIN; SELECT 13553; COMMIT;
DELETE FROM bench_t_18 WHERE id = 2;
SELECT [bracket_13555] FROM [dbo].[tbl_35];
/* block header 13556 */
UPDATE bench_t_53 SET payload = 13557 WHERE id = 21;
BEGIN; SELECT 13558; COMMIT;
WITH cte_13559 AS (SELECT 13559 AS n) SELECT n FROM cte_13559;
-- line 13560: deterministic comment
SELECT 13561 AS id, 'row_13561' AS label;
SELECT 13562 AS id, 'row_13562' AS label;
SELECT 13563 AS id, 'row_13563' AS label;
# hash comment 13564
SELECT * FROM "quoted_13565" WHERE col = E'esc\'13565';
SELECT `mysql_13566` FROM `tbl_16`;
WITH cte_13567 AS (SELECT 13567 AS n) SELECT n FROM cte_13567;
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
-- line 13569: deterministic comment
INSERT INTO bench_t_2 (id, payload) VALUES (13570, 'v13570');
SELECT * FROM "quoted_13571" WHERE col = E'esc\'13571';
# hash comment 13572
BEGIN; SELECT 13573; COMMIT;
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
BEGIN; SELECT 13575; COMMIT;
UPDATE bench_t_8 SET payload = 13576 WHERE id = 8;
$dz$ dollar body 13577 ; semicolon inside $dz$
SELECT nested FROM t WHERE id IN (13578, 13579, 13580);
# hash comment 13579
UPDATE bench_t_12 SET payload = 13580 WHERE id = 12;
UPDATE bench_t_13 SET payload = 13581 WHERE id = 13;
$dz$ dollar body 13582 ; semicolon inside $dz$
# hash comment 13583
DELETE FROM bench_t_16 WHERE id = 0;
WITH cte_13585 AS (SELECT 13585 AS n) SELECT n FROM cte_13585;
$dz$ dollar body 13586 ; semicolon inside $dz$
UPDATE bench_t_19 SET payload = 13587 WHERE id = 19;
BEGIN; SELECT 13588; COMMIT;
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
INSERT INTO bench_t_22 (id, payload) VALUES (13590, 'v13590');
UPDATE bench_t_23 SET payload = 13591 WHERE id = 23;
WITH cte_13592 AS (SELECT 13592 AS n) SELECT n FROM cte_13592;
DELETE FROM bench_t_25 WHERE id = 9;
# hash comment 13594
SELECT [bracket_13595] FROM [dbo].[tbl_35];
SELECT nested FROM t WHERE id IN (13596, 13597, 13598);
SELECT [bracket_13597] FROM [dbo].[tbl_37];
-- line 13598: deterministic comment
SELECT * FROM "quoted_13599" WHERE col = E'esc\'13599';
UPDATE bench_t_32 SET payload = 13600 WHERE id = 0;
SELECT [bracket_13601] FROM [dbo].[tbl_1];
INSERT INTO bench_t_34 (id, payload) VALUES (13602, 'v13602');
SELECT nested FROM t WHERE id IN (13603, 13604, 13605);
SELECT nested FROM t WHERE id IN (13604, 13605, 13606);
DELETE FROM bench_t_5 WHERE id = 5;
INSERT INTO bench_t_38 (id, payload) VALUES (13606, 'v13606');
SELECT nested FROM t WHERE id IN (13607, 13608, 13609);
UPDATE bench_t_40 SET payload = 13608 WHERE id = 8;
/* block header 13609 */
INSERT INTO bench_t_42 (id, payload) VALUES (13610, 'v13610');
SELECT `mysql_13611` FROM `tbl_11`;
/* block header 13612 */
SELECT nested FROM t WHERE id IN (13613, 13614, 13615);
SELECT 13614 AS id, 'row_13614' AS label;
SELECT nested FROM t WHERE id IN (13615, 13616, 13617);
INSERT INTO bench_t_48 (id, payload) VALUES (13616, 'v13616');
# hash comment 13617
# hash comment 13618
SELECT nested FROM t WHERE id IN (13619, 13620, 13621);
WITH cte_13620 AS (SELECT 13620 AS n) SELECT n FROM cte_13620;
DELETE FROM bench_t_21 WHERE id = 5;
-- line 13622: deterministic comment
DELETE FROM bench_t_23 WHERE id = 7;
-- line 13624: deterministic comment
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT nested FROM t WHERE id IN (13626, 13627, 13628);
SELECT 13627 AS id, 'row_13627' AS label;
INSERT INTO bench_t_60 (id, payload) VALUES (13628, 'v13628');
SELECT * FROM "quoted_13629" WHERE col = E'esc\'13629';
/* block header 13630 */
$dz$ dollar body 13631 ; semicolon inside $dz$
SELECT [bracket_13632] FROM [dbo].[tbl_32];
DELETE FROM bench_t_1 WHERE id = 1;
SELECT nested FROM t WHERE id IN (13634, 13635, 13636);
SELECT `mysql_13635` FROM `tbl_35`;
BEGIN; SELECT 13636; COMMIT;
SELECT * FROM "quoted_13637" WHERE col = E'esc\'13637';
# hash comment 13638
DELETE FROM bench_t_7 WHERE id = 7;
SELECT * FROM "quoted_13640" WHERE col = E'esc\'13640';
SELECT [bracket_13641] FROM [dbo].[tbl_1];
# hash comment 13642
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT * FROM "quoted_13644" WHERE col = E'esc\'13644';
SELECT 13645 AS id, 'row_13645' AS label;
SELECT 13646 AS id, 'row_13646' AS label;
DELETE FROM bench_t_15 WHERE id = 15;
-- line 13648: deterministic comment
/* block header 13649 */
SELECT nested FROM t WHERE id IN (13650, 13651, 13652);
UPDATE bench_t_19 SET payload = 13651 WHERE id = 19;
INSERT INTO bench_t_84 (id, payload) VALUES (13652, 'v13652');
SELECT nested FROM t WHERE id IN (13653, 13654, 13655);
UPDATE bench_t_22 SET payload = 13654 WHERE id = 22;
SELECT [bracket_13655] FROM [dbo].[tbl_15];
-- line 13656: deterministic comment
SELECT * FROM "quoted_13657" WHERE col = E'esc\'13657';
SELECT * FROM "quoted_13658" WHERE col = E'esc\'13658';
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT 13660 AS id, 'row_13660' AS label;
-- line 13661: deterministic comment
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT `mysql_13663` FROM `tbl_13`;
-- line 13664: deterministic comment
SELECT `mysql_13665` FROM `tbl_15`;
SELECT nested FROM t WHERE id IN (13666, 13667, 13668);
SELECT 13667 AS id, 'row_13667' AS label;
SELECT nested FROM t WHERE id IN (13668, 13669, 13670);
SELECT nested FROM t WHERE id IN (13669, 13670, 13671);
DELETE FROM bench_t_6 WHERE id = 6;
SELECT [bracket_13671] FROM [dbo].[tbl_31];
BEGIN; SELECT 13672; COMMIT;
SELECT `mysql_13673` FROM `tbl_23`;
WITH cte_13674 AS (SELECT 13674 AS n) SELECT n FROM cte_13674;
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT nested FROM t WHERE id IN (13677, 13678, 13679);
$dz$ dollar body 13678 ; semicolon inside $dz$
INSERT INTO bench_t_111 (id, payload) VALUES (13679, 'v13679');
$dz$ dollar body 13680 ; semicolon inside $dz$
SELECT * FROM "quoted_13681" WHERE col = E'esc\'13681';
SELECT `mysql_13682` FROM `tbl_32`;
$dz$ dollar body 13683 ; semicolon inside $dz$
/* block header 13684 */
$dz$ dollar body 13685 ; semicolon inside $dz$
DELETE FROM bench_t_22 WHERE id = 6;
INSERT INTO bench_t_119 (id, payload) VALUES (13687, 'v13687');
SELECT [bracket_13688] FROM [dbo].[tbl_8];
-- line 13689: deterministic comment
SELECT 13690 AS id, 'row_13690' AS label;
/* block header 13691 */
-- line 13692: deterministic comment
SELECT 13693 AS id, 'row_13693' AS label;
SELECT * FROM "quoted_13694" WHERE col = E'esc\'13694';
SELECT 13695 AS id, 'row_13695' AS label;
$dz$ dollar body 13696 ; semicolon inside $dz$
SELECT 13697 AS id, 'row_13697' AS label;
# hash comment 13698
SELECT `mysql_13699` FROM `tbl_49`;
UPDATE bench_t_4 SET payload = 13700 WHERE id = 4;
# hash comment 13701
-- line 13702: deterministic comment
SELECT 13703 AS id, 'row_13703' AS label;
SELECT nested FROM t WHERE id IN (13704, 13705, 13706);
/* block header 13705 */
$dz$ dollar body 13706 ; semicolon inside $dz$
UPDATE bench_t_11 SET payload = 13707 WHERE id = 11;
BEGIN; SELECT 13708; COMMIT;
UPDATE bench_t_13 SET payload = 13709 WHERE id = 13;
SELECT `mysql_13710` FROM `tbl_10`;
INSERT INTO bench_t_15 (id, payload) VALUES (13711, 'v13711');
SELECT * FROM "quoted_13712" WHERE col = E'esc\'13712';
WITH cte_13713 AS (SELECT 13713 AS n) SELECT n FROM cte_13713;
$dz$ dollar body 13714 ; semicolon inside $dz$
-- line 13715: deterministic comment
SELECT `mysql_13716` FROM `tbl_16`;
BEGIN; SELECT 13717; COMMIT;
$dz$ dollar body 13718 ; semicolon inside $dz$
# hash comment 13719
DELETE FROM bench_t_24 WHERE id = 8;
# hash comment 13721
SELECT 13722 AS id, 'row_13722' AS label;
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
$dz$ dollar body 13724 ; semicolon inside $dz$
DELETE FROM bench_t_29 WHERE id = 13;
SELECT [bracket_13726] FROM [dbo].[tbl_6];
SELECT [bracket_13727] FROM [dbo].[tbl_7];
SELECT [bracket_13728] FROM [dbo].[tbl_8];
UPDATE bench_t_33 SET payload = 13729 WHERE id = 1;
UPDATE bench_t_34 SET payload = 13730 WHERE id = 2;
SELECT nested FROM t WHERE id IN (13731, 13732, 13733);
UPDATE bench_t_36 SET payload = 13732 WHERE id = 4;
$dz$ dollar body 13733 ; semicolon inside $dz$
SELECT 13734 AS id, 'row_13734' AS label;
/* block header 13735 */
/* block header 13736 */
DELETE FROM bench_t_9 WHERE id = 9;
# hash comment 13738
# hash comment 13739
# hash comment 13740
/* block header 13741 */
/* block header 13742 */
INSERT INTO bench_t_47 (id, payload) VALUES (13743, 'v13743');
/* block header 13744 */
# hash comment 13745
SELECT 13746 AS id, 'row_13746' AS label;
SELECT 13747 AS id, 'row_13747' AS label;
/* block header 13748 */
# hash comment 13749
/*
 * section 55
 * checksum 7a33
 */
SELECT nested FROM t WHERE id IN (13750, 13751, 13752);
-- line 13755: deterministic comment
SELECT [bracket_13756] FROM [dbo].[tbl_36];
-- line 13757: deterministic comment
$dz$ dollar body 13758 ; semicolon inside $dz$
$dz$ dollar body 13759 ; semicolon inside $dz$
DELETE FROM bench_t_0 WHERE id = 0;
INSERT INTO bench_t_65 (id, payload) VALUES (13761, 'O''Brien');
# hash comment 13762
SELECT [bracket_13763] FROM [dbo].[tbl_3];
SELECT `mysql_13764` FROM `tbl_14`;
SELECT nested FROM t WHERE id IN (13765, 13766, 13767);
$dz$ dollar body 13766 ; semicolon inside $dz$
# hash comment 13767
SELECT 13768 AS id, 'row_13768' AS label;
UPDATE bench_t_9 SET payload = 13769 WHERE id = 9;
UPDATE bench_t_10 SET payload = 13770 WHERE id = 10;
SELECT [bracket_13771] FROM [dbo].[tbl_11];
SELECT * FROM "quoted_13772" WHERE col = E'esc\'13772';
# hash comment 13773
SELECT [bracket_13774] FROM [dbo].[tbl_14];
$dz$ dollar body 13775 ; semicolon inside $dz$
SELECT 13776 AS id, 'row_13776' AS label;
SELECT nested FROM t WHERE id IN (13777, 13778, 13779);
SELECT nested FROM t WHERE id IN (13778, 13779, 13780);
SELECT nested FROM t WHERE id IN (13779, 13780, 13781);
BEGIN; SELECT 13780; COMMIT;
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT `mysql_13782` FROM `tbl_32`;
SELECT * FROM "quoted_13783" WHERE col = E'esc\'13783';
UPDATE bench_t_24 SET payload = 13784 WHERE id = 24;
INSERT INTO bench_t_89 (id, payload) VALUES (13785, 'v13785');
BEGIN; SELECT 13786; COMMIT;
SELECT 13787 AS id, 'row_13787' AS label;
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
INSERT INTO bench_t_93 (id, payload) VALUES (13789, 'v13789');
SELECT [bracket_13790] FROM [dbo].[tbl_30];
SELECT nested FROM t WHERE id IN (13791, 13792, 13793);
INSERT INTO bench_t_96 (id, payload) VALUES (13792, 'v13792');
SELECT `mysql_13793` FROM `tbl_43`;
BEGIN; SELECT 13794; COMMIT;
$dz$ dollar body 13795 ; semicolon inside $dz$
BEGIN; SELECT 13796; COMMIT;
DELETE FROM bench_t_5 WHERE id = 5;
INSERT INTO bench_t_102 (id, payload) VALUES (13798, 'v13798');
SELECT * FROM "quoted_13799" WHERE col = E'esc\'13799';
UPDATE bench_t_40 SET payload = 13800 WHERE id = 8;
SELECT `mysql_13801` FROM `tbl_1`;
SELECT [bracket_13802] FROM [dbo].[tbl_2];
SELECT nested FROM t WHERE id IN (13803, 13804, 13805);
DELETE FROM bench_t_12 WHERE id = 12;
-- line 13805: deterministic comment
SELECT * FROM "quoted_13806" WHERE col = E'esc\'13806';
SELECT * FROM "quoted_13807" WHERE col = E'esc\'13807';
$dz$ dollar body 13808 ; semicolon inside $dz$
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
BEGIN; SELECT 13810; COMMIT;
/* block header 13811 */
INSERT INTO bench_t_116 (id, payload) VALUES (13812, 'v13812');
SELECT [bracket_13813] FROM [dbo].[tbl_13];
SELECT 13814 AS id, 'row_13814' AS label;
SELECT `mysql_13815` FROM `tbl_15`;
SELECT nested FROM t WHERE id IN (13816, 13817, 13818);
SELECT nested FROM t WHERE id IN (13817, 13818, 13819);
-- line 13818: deterministic comment
BEGIN; SELECT 13819; COMMIT;
DELETE FROM bench_t_28 WHERE id = 12;
INSERT INTO bench_t_125 (id, payload) VALUES (13821, 'v13821');
-- line 13822: deterministic comment
SELECT `mysql_13823` FROM `tbl_23`;
UPDATE bench_t_0 SET payload = 13824 WHERE id = 0;
SELECT nested FROM t WHERE id IN (13825, 13826, 13827);
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
INSERT INTO bench_t_3 (id, payload) VALUES (13827, 'O''Brien');
/* block header 13828 */
INSERT INTO bench_t_5 (id, payload) VALUES (13829, 'v13829');
SELECT * FROM "quoted_13830" WHERE col = E'esc\'13830';
SELECT 13831 AS id, 'row_13831' AS label;
$dz$ dollar body 13832 ; semicolon inside $dz$
SELECT nested FROM t WHERE id IN (13833, 13834, 13835);
DELETE FROM bench_t_10 WHERE id = 10;
$dz$ dollar body 13835 ; semicolon inside $dz$
SELECT [bracket_13836] FROM [dbo].[tbl_36];
-- line 13837: deterministic comment
-- line 13838: deterministic comment
$dz$ dollar body 13839 ; semicolon inside $dz$
BEGIN; SELECT 13840; COMMIT;
SELECT `mysql_13841` FROM `tbl_41`;
SELECT 13842 AS id, 'row_13842' AS label;
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT nested FROM t WHERE id IN (13844, 13845, 13846);
SELECT * FROM "quoted_13845" WHERE col = E'esc\'13845';
-- line 13846: deterministic comment
BEGIN; SELECT 13847; COMMIT;
SELECT * FROM "quoted_13848" WHERE col = E'esc\'13848';
DELETE FROM bench_t_25 WHERE id = 9;
/* block header 13850 */
SELECT nested FROM t WHERE id IN (13851, 13852, 13853);
SELECT nested FROM t WHERE id IN (13852, 13853, 13854);
SELECT * FROM "quoted_13853" WHERE col = E'esc\'13853';
DELETE FROM bench_t_30 WHERE id = 14;
WITH cte_13855 AS (SELECT 13855 AS n) SELECT n FROM cte_13855;
DELETE FROM bench_t_0 WHERE id = 0;
# hash comment 13857
INSERT INTO bench_t_34 (id, payload) VALUES (13858, 'v13858');
INSERT INTO bench_t_35 (id, payload) VALUES (13859, 'v13859');
$dz$ dollar body 13860 ; semicolon inside $dz$
UPDATE bench_t_37 SET payload = 13861 WHERE id = 5;
INSERT INTO bench_t_38 (id, payload) VALUES (13862, 'v13862');
$dz$ dollar body 13863 ; semicolon inside $dz$
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
BEGIN; SELECT 13865; COMMIT;
$dz$ dollar body 13866 ; semicolon inside $dz$
UPDATE bench_t_43 SET payload = 13867 WHERE id = 11;
UPDATE bench_t_44 SET payload = 13868 WHERE id = 12;
SELECT 13869 AS id, 'row_13869' AS label;
$dz$ dollar body 13870 ; semicolon inside $dz$
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
UPDATE bench_t_48 SET payload = 13872 WHERE id = 16;
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT nested FROM t WHERE id IN (13874, 13875, 13876);
SELECT 13875 AS id, 'row_13875' AS label;
/* block header 13876 */
UPDATE bench_t_53 SET payload = 13877 WHERE id = 21;
SELECT [bracket_13878] FROM [dbo].[tbl_38];
/* block header 13879 */
SELECT 13880 AS id, 'row_13880' AS label;
$dz$ dollar body 13881 ; semicolon inside $dz$
UPDATE bench_t_58 SET payload = 13882 WHERE id = 26;
/* block header 13883 */
SELECT `mysql_13884` FROM `tbl_34`;
SELECT [bracket_13885] FROM [dbo].[tbl_5];
DELETE FROM bench_t_30 WHERE id = 14;
$dz$ dollar body 13887 ; semicolon inside $dz$
UPDATE bench_t_0 SET payload = 13888 WHERE id = 0;
WITH cte_13889 AS (SELECT 13889 AS n) SELECT n FROM cte_13889;
SELECT 13890 AS id, 'row_13890' AS label;
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
BEGIN; SELECT 13892; COMMIT;
/* block header 13893 */
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT [bracket_13895] FROM [dbo].[tbl_15];
SELECT nested FROM t WHERE id IN (13896, 13897, 13898);
DELETE FROM bench_t_9 WHERE id = 9;
# hash comment 13898
DELETE FROM bench_t_11 WHERE id = 11;
SELECT 13900 AS id, 'row_13900' AS label;
BEGIN; SELECT 13901; COMMIT;
UPDATE bench_t_14 SET payload = 13902 WHERE id = 14;
DELETE FROM bench_t_15 WHERE id = 15;
SELECT [bracket_13904] FROM [dbo].[tbl_24];
# hash comment 13905
SELECT nested FROM t WHERE id IN (13906, 13907, 13908);
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT [bracket_13908] FROM [dbo].[tbl_28];
-- line 13909: deterministic comment
INSERT INTO bench_t_86 (id, payload) VALUES (13910, 'v13910');
INSERT INTO bench_t_87 (id, payload) VALUES (13911, 'v13911');
UPDATE bench_t_24 SET payload = 13912 WHERE id = 24;
$dz$ dollar body 13913 ; semicolon inside $dz$
# hash comment 13914
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
BEGIN; SELECT 13916; COMMIT;
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT nested FROM t WHERE id IN (13919, 13920, 13921);
WITH cte_13920 AS (SELECT 13920 AS n) SELECT n FROM cte_13920;
INSERT INTO bench_t_97 (id, payload) VALUES (13921, 'v13921');
INSERT INTO bench_t_98 (id, payload) VALUES (13922, 'v13922');
SELECT `mysql_13923` FROM `tbl_23`;
# hash comment 13924
SELECT [bracket_13925] FROM [dbo].[tbl_5];
DELETE FROM bench_t_6 WHERE id = 6;
# hash comment 13927
INSERT INTO bench_t_104 (id, payload) VALUES (13928, 'v13928');
-- line 13929: deterministic comment
WITH cte_13930 AS (SELECT 13930 AS n) SELECT n FROM cte_13930;
-- line 13931: deterministic comment
UPDATE bench_t_44 SET payload = 13932 WHERE id = 12;
BEGIN; SELECT 13933; COMMIT;
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
DELETE FROM bench_t_15 WHERE id = 15;
SELECT [bracket_13936] FROM [dbo].[tbl_16];
# hash comment 13937
$dz$ dollar body 13938 ; semicolon inside $dz$
# hash comment 13939
SELECT * FROM "quoted_13940" WHERE col = E'esc\'13940';
WITH cte_13941 AS (SELECT 13941 AS n) SELECT n FROM cte_13941;
WITH cte_13942 AS (SELECT 13942 AS n) SELECT n FROM cte_13942;
$dz$ dollar body 13943 ; semicolon inside $dz$
SELECT nested FROM t WHERE id IN (13944, 13945, 13946);
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT `mysql_13946` FROM `tbl_46`;
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT `mysql_13948` FROM `tbl_48`;
BEGIN; SELECT 13949; COMMIT;
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
BEGIN; SELECT 13951; COMMIT;
-- line 13952: deterministic comment
DELETE FROM bench_t_1 WHERE id = 1;
UPDATE bench_t_2 SET payload = 13954 WHERE id = 2;
SELECT * FROM "quoted_13955" WHERE col = E'esc\'13955';
-- line 13956: deterministic comment
INSERT INTO bench_t_5 (id, payload) VALUES (13957, 'v13957');
SELECT [bracket_13958] FROM [dbo].[tbl_38];
/* block header 13959 */
$dz$ dollar body 13960 ; semicolon inside $dz$
UPDATE bench_t_9 SET payload = 13961 WHERE id = 9;
SELECT [bracket_13962] FROM [dbo].[tbl_2];
SELECT * FROM "quoted_13963" WHERE col = E'esc\'13963';
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
# hash comment 13965
DELETE FROM bench_t_14 WHERE id = 14;
SELECT [bracket_13967] FROM [dbo].[tbl_7];
SELECT nested FROM t WHERE id IN (13968, 13969, 13970);
$dz$ dollar body 13969 ; semicolon inside $dz$
SELECT [bracket_13970] FROM [dbo].[tbl_10];
WITH cte_13971 AS (SELECT 13971 AS n) SELECT n FROM cte_13971;
SELECT * FROM "quoted_13972" WHERE col = E'esc\'13972';
INSERT INTO bench_t_21 (id, payload) VALUES (13973, 'v13973');
SELECT nested FROM t WHERE id IN (13974, 13975, 13976);
WITH cte_13975 AS (SELECT 13975 AS n) SELECT n FROM cte_13975;
BEGIN; SELECT 13976; COMMIT;
WITH cte_13977 AS (SELECT 13977 AS n) SELECT n FROM cte_13977;
SELECT [bracket_13978] FROM [dbo].[tbl_18];
SELECT nested FROM t WHERE id IN (13979, 13980, 13981);
BEGIN; SELECT 13980; COMMIT;
DELETE FROM bench_t_29 WHERE id = 13;
SELECT * FROM "quoted_13982" WHERE col = E'esc\'13982';
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT `mysql_13984` FROM `tbl_34`;
SELECT `mysql_13985` FROM `tbl_35`;
-- line 13986: deterministic comment
BEGIN; SELECT 13987; COMMIT;
UPDATE bench_t_36 SET payload = 13988 WHERE id = 4;
# hash comment 13989
/* block header 13990 */
SELECT nested FROM t WHERE id IN (13991, 13992, 13993);
SELECT 13992 AS id, 'row_13992' AS label;
$dz$ dollar body 13993 ; semicolon inside $dz$
/* block header 13994 */
WITH cte_13995 AS (SELECT 13995 AS n) SELECT n FROM cte_13995;
# hash comment 13996
SELECT nested FROM t WHERE id IN (13997, 13998, 13999);
DELETE FROM bench_t_14 WHERE id = 14;
-- line 13999: deterministic comment
/*
 * section 56
 * checksum 3c57
 */
BEGIN; SELECT 14000; COMMIT;
SELECT 14005 AS id, 'row_14005' AS label;
UPDATE bench_t_54 SET payload = 14006 WHERE id = 22;
SELECT 14007 AS id, 'row_14007' AS label;
$dz$ dollar body 14008 ; semicolon inside $dz$
-- line 14009: deterministic comment
UPDATE bench_t_58 SET payload = 14010 WHERE id = 26;
BEGIN; SELECT 14011; COMMIT;
UPDATE bench_t_60 SET payload = 14012 WHERE id = 28;
# hash comment 14013
BEGIN; SELECT 14014; COMMIT;
BEGIN; SELECT 14015; COMMIT;
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT * FROM "quoted_14017" WHERE col = E'esc\'14017';
BEGIN; SELECT 14018; COMMIT;
UPDATE bench_t_3 SET payload = 14019 WHERE id = 3;
SELECT nested FROM t WHERE id IN (14020, 14021, 14022);
-- line 14021: deterministic comment
SELECT nested FROM t WHERE id IN (14022, 14023, 14024);
SELECT [bracket_14023] FROM [dbo].[tbl_23];
INSERT INTO bench_t_72 (id, payload) VALUES (14024, 'v14024');
/* block header 14025 */
BEGIN; SELECT 14026; COMMIT;
BEGIN; SELECT 14027; COMMIT;
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT `mysql_14031` FROM `tbl_31`;
BEGIN; SELECT 14032; COMMIT;
SELECT * FROM "quoted_14033" WHERE col = E'esc\'14033';
BEGIN; SELECT 14034; COMMIT;
-- line 14035: deterministic comment
SELECT nested FROM t WHERE id IN (14036, 14037, 14038);
SELECT [bracket_14037] FROM [dbo].[tbl_37];
# hash comment 14038
SELECT 14039 AS id, 'row_14039' AS label;
SELECT nested FROM t WHERE id IN (14040, 14041, 14042);
INSERT INTO bench_t_89 (id, payload) VALUES (14041, 'v14041');
-- line 14042: deterministic comment
WITH cte_14043 AS (SELECT 14043 AS n) SELECT n FROM cte_14043;
WITH cte_14044 AS (SELECT 14044 AS n) SELECT n FROM cte_14044;
SELECT * FROM "quoted_14045" WHERE col = E'esc\'14045';
WITH cte_14046 AS (SELECT 14046 AS n) SELECT n FROM cte_14046;
DELETE FROM bench_t_31 WHERE id = 15;
SELECT [bracket_14048] FROM [dbo].[tbl_8];
DELETE FROM bench_t_1 WHERE id = 1;
WITH cte_14050 AS (SELECT 14050 AS n) SELECT n FROM cte_14050;
UPDATE bench_t_35 SET payload = 14051 WHERE id = 3;
SELECT [bracket_14052] FROM [dbo].[tbl_12];
SELECT nested FROM t WHERE id IN (14053, 14054, 14055);
SELECT * FROM "quoted_14054" WHERE col = E'esc\'14054';
SELECT nested FROM t WHERE id IN (14055, 14056, 14057);
SELECT 14056 AS id, 'row_14056' AS label;
$dz$ dollar body 14057 ; semicolon inside $dz$
SELECT 14058 AS id, 'row_14058' AS label;
-- line 14059: deterministic comment
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
# hash comment 14061
SELECT * FROM "quoted_14062" WHERE col = E'esc\'14062';
DELETE FROM bench_t_15 WHERE id = 15;
SELECT * FROM "quoted_14064" WHERE col = E'esc\'14064';
INSERT INTO bench_t_113 (id, payload) VALUES (14065, 'v14065');
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
INSERT INTO bench_t_115 (id, payload) VALUES (14067, 'v14067');
BEGIN; SELECT 14068; COMMIT;
DELETE FROM bench_t_21 WHERE id = 5;
SELECT nested FROM t WHERE id IN (14070, 14071, 14072);
UPDATE bench_t_55 SET payload = 14071 WHERE id = 23;
SELECT 14072 AS id, 'row_14072' AS label;
/* block header 14073 */
-- line 14074: deterministic comment
DELETE FROM bench_t_27 WHERE id = 11;
DELETE FROM bench_t_28 WHERE id = 12;
BEGIN; SELECT 14077; COMMIT;
SELECT `mysql_14078` FROM `tbl_28`;
# hash comment 14079
SELECT [bracket_14080] FROM [dbo].[tbl_0];
/* block header 14081 */
/* block header 14082 */
UPDATE bench_t_3 SET payload = 14083 WHERE id = 3;
SELECT * FROM "quoted_14084" WHERE col = E'esc\'14084';
INSERT INTO bench_t_5 (id, payload) VALUES (14085, 'v14085');
-- line 14086: deterministic comment
UPDATE bench_t_7 SET payload = 14087 WHERE id = 7;
UPDATE bench_t_8 SET payload = 14088 WHERE id = 8;
-- line 14089: deterministic comment
SELECT * FROM "quoted_14090" WHERE col = E'esc\'14090';
/* block header 14091 */
SELECT nested FROM t WHERE id IN (14092, 14093, 14094);
$dz$ dollar body 14093 ; semicolon inside $dz$
SELECT `mysql_14094` FROM `tbl_44`;
$dz$ dollar body 14095 ; semicolon inside $dz$
-- line 14096: deterministic comment
BEGIN; SELECT 14097; COMMIT;
# hash comment 14098
SELECT `mysql_14099` FROM `tbl_49`;
SELECT nested FROM t WHERE id IN (14100, 14101, 14102);
WITH cte_14101 AS (SELECT 14101 AS n) SELECT n FROM cte_14101;
SELECT [bracket_14102] FROM [dbo].[tbl_22];
SELECT `mysql_14103` FROM `tbl_3`;
/* block header 14104 */
SELECT [bracket_14105] FROM [dbo].[tbl_25];
/* block header 14106 */
UPDATE bench_t_27 SET payload = 14107 WHERE id = 27;
SELECT nested FROM t WHERE id IN (14108, 14109, 14110);
SELECT `mysql_14109` FROM `tbl_9`;
SELECT nested FROM t WHERE id IN (14110, 14111, 14112);
SELECT [bracket_14111] FROM [dbo].[tbl_31];
SELECT 14112 AS id, 'row_14112' AS label;
# hash comment 14113
SELECT `mysql_14114` FROM `tbl_14`;
SELECT nested FROM t WHERE id IN (14115, 14116, 14117);
SELECT nested FROM t WHERE id IN (14116, 14117, 14118);
SELECT `mysql_14117` FROM `tbl_17`;
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT nested FROM t WHERE id IN (14119, 14120, 14121);
SELECT [bracket_14120] FROM [dbo].[tbl_0];
SELECT nested FROM t WHERE id IN (14121, 14122, 14123);
SELECT nested FROM t WHERE id IN (14122, 14123, 14124);
SELECT nested FROM t WHERE id IN (14123, 14124, 14125);
# hash comment 14124
WITH cte_14125 AS (SELECT 14125 AS n) SELECT n FROM cte_14125;
SELECT `mysql_14126` FROM `tbl_26`;
SELECT nested FROM t WHERE id IN (14127, 14128, 14129);
INSERT INTO bench_t_48 (id, payload) VALUES (14128, 'v14128');
SELECT `mysql_14129` FROM `tbl_29`;
SELECT nested FROM t WHERE id IN (14130, 14131, 14132);
SELECT [bracket_14131] FROM [dbo].[tbl_11];
SELECT `mysql_14132` FROM `tbl_32`;
SELECT nested FROM t WHERE id IN (14133, 14134, 14135);
/* block header 14134 */
$dz$ dollar body 14135 ; semicolon inside $dz$
UPDATE bench_t_56 SET payload = 14136 WHERE id = 24;
INSERT INTO bench_t_57 (id, payload) VALUES (14137, 'v14137');
SELECT nested FROM t WHERE id IN (14138, 14139, 14140);
INSERT INTO bench_t_59 (id, payload) VALUES (14139, 'v14139');
UPDATE bench_t_60 SET payload = 14140 WHERE id = 28;
$dz$ dollar body 14141 ; semicolon inside $dz$
SELECT 14142 AS id, 'row_14142' AS label;
UPDATE bench_t_63 SET payload = 14143 WHERE id = 31;
SELECT 14144 AS id, 'row_14144' AS label;
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
DELETE FROM bench_t_2 WHERE id = 2;
SELECT * FROM "quoted_14147" WHERE col = E'esc\'14147';
SELECT * FROM "quoted_14148" WHERE col = E'esc\'14148';
/* block header 14149 */
SELECT nested FROM t WHERE id IN (14150, 14151, 14152);
SELECT nested FROM t WHERE id IN (14151, 14152, 14153);
WITH cte_14152 AS (SELECT 14152 AS n) SELECT n FROM cte_14152;
-- line 14153: deterministic comment
SELECT 14154 AS id, 'row_14154' AS label;
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
INSERT INTO bench_t_76 (id, payload) VALUES (14156, 'v14156');
SELECT nested FROM t WHERE id IN (14157, 14158, 14159);
BEGIN; SELECT 14158; COMMIT;
SELECT [bracket_14159] FROM [dbo].[tbl_39];
# hash comment 14160
SELECT [bracket_14161] FROM [dbo].[tbl_1];
SELECT [bracket_14162] FROM [dbo].[tbl_2];
-- line 14163: deterministic comment
/* block header 14164 */
BEGIN; SELECT 14165; COMMIT;
SELECT `mysql_14166` FROM `tbl_16`;
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT * FROM "quoted_14168" WHERE col = E'esc\'14168';
DELETE FROM bench_t_25 WHERE id = 9;
UPDATE bench_t_26 SET payload = 14170 WHERE id = 26;
DELETE FROM bench_t_27 WHERE id = 11;
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
INSERT INTO bench_t_93 (id, payload) VALUES (14173, 'v14173');
DELETE FROM bench_t_30 WHERE id = 14;
SELECT * FROM "quoted_14175" WHERE col = E'esc\'14175';
SELECT nested FROM t WHERE id IN (14176, 14177, 14178);
DELETE FROM bench_t_1 WHERE id = 1;
/* block header 14178 */
/* block header 14179 */
BEGIN; SELECT 14180; COMMIT;
$dz$ dollar body 14181 ; semicolon inside $dz$
INSERT INTO bench_t_102 (id, payload) VALUES (14182, 'v14182');
WITH cte_14183 AS (SELECT 14183 AS n) SELECT n FROM cte_14183;
$dz$ dollar body 14184 ; semicolon inside $dz$
DELETE FROM bench_t_9 WHERE id = 9;
/* block header 14186 */
SELECT nested FROM t WHERE id IN (14187, 14188, 14189);
DELETE FROM bench_t_12 WHERE id = 12;
SELECT nested FROM t WHERE id IN (14189, 14190, 14191);
UPDATE bench_t_46 SET payload = 14190 WHERE id = 14;
SELECT * FROM "quoted_14191" WHERE col = E'esc\'14191';
UPDATE bench_t_48 SET payload = 14192 WHERE id = 16;
SELECT * FROM "quoted_14193" WHERE col = E'esc\'14193';
SELECT nested FROM t WHERE id IN (14194, 14195, 14196);
INSERT INTO bench_t_115 (id, payload) VALUES (14195, 'v14195');
SELECT 14196 AS id, 'row_14196' AS label;
INSERT INTO bench_t_117 (id, payload) VALUES (14197, 'v14197');
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT nested FROM t WHERE id IN (14199, 14200, 14201);
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
UPDATE bench_t_57 SET payload = 14201 WHERE id = 25;
/* block header 14202 */
SELECT 14203 AS id, 'row_14203' AS label;
# hash comment 14204
# hash comment 14205
# hash comment 14206
# hash comment 14207
SELECT * FROM "quoted_14208" WHERE col = E'esc\'14208';
DELETE FROM bench_t_1 WHERE id = 1;
$dz$ dollar body 14210 ; semicolon inside $dz$
INSERT INTO bench_t_3 (id, payload) VALUES (14211, 'v14211');
SELECT `mysql_14212` FROM `tbl_12`;
UPDATE bench_t_5 SET payload = 14213 WHERE id = 5;
# hash comment 14214
$dz$ dollar body 14215 ; semicolon inside $dz$
SELECT 14216 AS id, 'row_14216' AS label;
SELECT 14217 AS id, 'row_14217' AS label;
SELECT nested FROM t WHERE id IN (14218, 14219, 14220);
SELECT `mysql_14219` FROM `tbl_19`;
SELECT `mysql_14220` FROM `tbl_20`;
SELECT * FROM "quoted_14221" WHERE col = E'esc\'14221';
# hash comment 14222
-- line 14223: deterministic comment
SELECT `mysql_14224` FROM `tbl_24`;
/* block header 14225 */
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT `mysql_14227` FROM `tbl_27`;
DELETE FROM bench_t_20 WHERE id = 4;
$dz$ dollar body 14229 ; semicolon inside $dz$
$dz$ dollar body 14230 ; semicolon inside $dz$
UPDATE bench_t_23 SET payload = 14231 WHERE id = 23;
DELETE FROM bench_t_24 WHERE id = 8;
SELECT [bracket_14233] FROM [dbo].[tbl_33];
# hash comment 14234
INSERT INTO bench_t_27 (id, payload) VALUES (14235, 'v14235');
/* block header 14236 */
INSERT INTO bench_t_29 (id, payload) VALUES (14237, 'v14237');
SELECT * FROM "quoted_14238" WHERE col = E'esc\'14238';
-- line 14239: deterministic comment
SELECT 14240 AS id, 'row_14240' AS label;
$dz$ dollar body 14241 ; semicolon inside $dz$
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT `mysql_14243` FROM `tbl_43`;
$dz$ dollar body 14244 ; semicolon inside $dz$
DELETE FROM bench_t_5 WHERE id = 5;
UPDATE bench_t_38 SET payload = 14246 WHERE id = 6;
SELECT 14247 AS id, 'row_14247' AS label;
-- line 14248: deterministic comment
WITH cte_14249 AS (SELECT 14249 AS n) SELECT n FROM cte_14249;
/*
 * section 57
 * checksum 5a41
 */
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
UPDATE bench_t_47 SET payload = 14255 WHERE id = 15;
SELECT nested FROM t WHERE id IN (14256, 14257, 14258);
$dz$ dollar body 14257 ; semicolon inside $dz$
SELECT * FROM "quoted_14258" WHERE col = E'esc\'14258';
# hash comment 14259
SELECT [bracket_14260] FROM [dbo].[tbl_20];
SELECT `mysql_14261` FROM `tbl_11`;
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
$dz$ dollar body 14263 ; semicolon inside $dz$
UPDATE bench_t_56 SET payload = 14264 WHERE id = 24;
SELECT nested FROM t WHERE id IN (14265, 14266, 14267);
SELECT 14266 AS id, 'row_14266' AS label;
UPDATE bench_t_59 SET payload = 14267 WHERE id = 27;
SELECT nested FROM t WHERE id IN (14268, 14269, 14270);
DELETE FROM bench_t_29 WHERE id = 13;
WITH cte_14270 AS (SELECT 14270 AS n) SELECT n FROM cte_14270;
# hash comment 14271
SELECT nested FROM t WHERE id IN (14272, 14273, 14274);
-- line 14273: deterministic comment
-- line 14274: deterministic comment
SELECT nested FROM t WHERE id IN (14275, 14276, 14277);
DELETE FROM bench_t_4 WHERE id = 4;
SELECT * FROM "quoted_14277" WHERE col = E'esc\'14277';
WITH cte_14278 AS (SELECT 14278 AS n) SELECT n FROM cte_14278;
WITH cte_14279 AS (SELECT 14279 AS n) SELECT n FROM cte_14279;
INSERT INTO bench_t_72 (id, payload) VALUES (14280, 'v14280');
SELECT `mysql_14281` FROM `tbl_31`;
WITH cte_14282 AS (SELECT 14282 AS n) SELECT n FROM cte_14282;
UPDATE bench_t_11 SET payload = 14283 WHERE id = 11;
$dz$ dollar body 14284 ; semicolon inside $dz$
SELECT nested FROM t WHERE id IN (14285, 14286, 14287);
SELECT `mysql_14286` FROM `tbl_36`;
SELECT [bracket_14287] FROM [dbo].[tbl_7];
INSERT INTO bench_t_80 (id, payload) VALUES (14288, 'v14288');
BEGIN; SELECT 14289; COMMIT;
SELECT 14290 AS id, 'row_14290' AS label;
DELETE FROM bench_t_19 WHERE id = 3;
BEGIN; SELECT 14292; COMMIT;
SELECT * FROM "quoted_14293" WHERE col = E'esc\'14293';
SELECT 14294 AS id, 'row_14294' AS label;
SELECT `mysql_14295` FROM `tbl_45`;
INSERT INTO bench_t_88 (id, payload) VALUES (14296, 'v14296');
-- line 14297: deterministic comment
WITH cte_14298 AS (SELECT 14298 AS n) SELECT n FROM cte_14298;
# hash comment 14299
INSERT INTO bench_t_92 (id, payload) VALUES (14300, 'O''Brien');
SELECT `mysql_14301` FROM `tbl_1`;
-- line 14302: deterministic comment
SELECT 14303 AS id, 'row_14303' AS label;
SELECT `mysql_14304` FROM `tbl_4`;
SELECT * FROM "quoted_14305" WHERE col = E'esc\'14305';
SELECT [bracket_14306] FROM [dbo].[tbl_26];
/* block header 14307 */
WITH cte_14308 AS (SELECT 14308 AS n) SELECT n FROM cte_14308;
# hash comment 14309
DELETE FROM bench_t_6 WHERE id = 6;
BEGIN; SELECT 14311; COMMIT;
-- line 14312: deterministic comment
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
BEGIN; SELECT 14314; COMMIT;
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
WITH cte_14316 AS (SELECT 14316 AS n) SELECT n FROM cte_14316;
BEGIN; SELECT 14317; COMMIT;
SELECT 14318 AS id, 'row_14318' AS label;
$dz$ dollar body 14319 ; semicolon inside $dz$
SELECT [bracket_14320] FROM [dbo].[tbl_0];
SELECT [bracket_14321] FROM [dbo].[tbl_1];
DELETE FROM bench_t_18 WHERE id = 2;
SELECT * FROM "quoted_14323" WHERE col = E'esc\'14323';
WITH cte_14324 AS (SELECT 14324 AS n) SELECT n FROM cte_14324;
SELECT nested FROM t WHERE id IN (14325, 14326, 14327);
-- line 14326: deterministic comment
UPDATE bench_t_55 SET payload = 14327 WHERE id = 23;
UPDATE bench_t_56 SET payload = 14328 WHERE id = 24;
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT `mysql_14330` FROM `tbl_30`;
BEGIN; SELECT 14331; COMMIT;
WITH cte_14332 AS (SELECT 14332 AS n) SELECT n FROM cte_14332;
SELECT * FROM "quoted_14333" WHERE col = E'esc\'14333';
BEGIN; SELECT 14334; COMMIT;
SELECT `mysql_14335` FROM `tbl_35`;
DELETE FROM bench_t_0 WHERE id = 0;
INSERT INTO bench_t_1 (id, payload) VALUES (14337, 'v14337');
SELECT nested FROM t WHERE id IN (14338, 14339, 14340);
-- line 14339: deterministic comment
UPDATE bench_t_4 SET payload = 14340 WHERE id = 4;
INSERT INTO bench_t_5 (id, payload) VALUES (14341, 'v14341');
SELECT nested FROM t WHERE id IN (14342, 14343, 14344);
INSERT INTO bench_t_7 (id, payload) VALUES (14343, 'v14343');
BEGIN; SELECT 14344; COMMIT;
WITH cte_14345 AS (SELECT 14345 AS n) SELECT n FROM cte_14345;
WITH cte_14346 AS (SELECT 14346 AS n) SELECT n FROM cte_14346;
UPDATE bench_t_11 SET payload = 14347 WHERE id = 11;
-- line 14348: deterministic comment
SELECT `mysql_14349` FROM `tbl_49`;
BEGIN; SELECT 14350; COMMIT;
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
WITH cte_14352 AS (SELECT 14352 AS n) SELECT n FROM cte_14352;
SELECT `mysql_14353` FROM `tbl_3`;
-- line 14354: deterministic comment
-- line 14355: deterministic comment
SELECT `mysql_14356` FROM `tbl_6`;
SELECT [bracket_14357] FROM [dbo].[tbl_37];
INSERT INTO bench_t_22 (id, payload) VALUES (14358, 'v14358');
BEGIN; SELECT 14359; COMMIT;
SELECT `mysql_14360` FROM `tbl_10`;
INSERT INTO bench_t_25 (id, payload) VALUES (14361, 'v14361');
SELECT nested FROM t WHERE id IN (14362, 14363, 14364);
SELECT nested FROM t WHERE id IN (14363, 14364, 14365);
WITH cte_14364 AS (SELECT 14364 AS n) SELECT n FROM cte_14364;
/* block header 14365 */
SELECT [bracket_14366] FROM [dbo].[tbl_6];
# hash comment 14367
/* block header 14368 */
SELECT 14369 AS id, 'row_14369' AS label;
WITH cte_14370 AS (SELECT 14370 AS n) SELECT n FROM cte_14370;
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT 14373 AS id, 'row_14373' AS label;
SELECT * FROM "quoted_14374" WHERE col = E'esc\'14374';
/* block header 14375 */
SELECT 14376 AS id, 'row_14376' AS label;
SELECT `mysql_14377` FROM `tbl_27`;
SELECT [bracket_14378] FROM [dbo].[tbl_18];
BEGIN; SELECT 14379; COMMIT;
WITH cte_14380 AS (SELECT 14380 AS n) SELECT n FROM cte_14380;
DELETE FROM bench_t_13 WHERE id = 13;
BEGIN; SELECT 14382; COMMIT;
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
# hash comment 14384
DELETE FROM bench_t_17 WHERE id = 1;
UPDATE bench_t_50 SET payload = 14386 WHERE id = 18;
WITH cte_14387 AS (SELECT 14387 AS n) SELECT n FROM cte_14387;
BEGIN; SELECT 14388; COMMIT;
/* block header 14389 */
SELECT 14390 AS id, 'row_14390' AS label;
DELETE FROM bench_t_23 WHERE id = 7;
SELECT [bracket_14392] FROM [dbo].[tbl_32];
SELECT nested FROM t WHERE id IN (14393, 14394, 14395);
SELECT [bracket_14394] FROM [dbo].[tbl_34];
SELECT nested FROM t WHERE id IN (14395, 14396, 14397);
WITH cte_14396 AS (SELECT 14396 AS n) SELECT n FROM cte_14396;
UPDATE bench_t_61 SET payload = 14397 WHERE id = 29;
/* block header 14398 */
/* block header 14399 */
SELECT 14400 AS id, 'row_14400' AS label;
DELETE FROM bench_t_1 WHERE id = 1;
SELECT `mysql_14402` FROM `tbl_2`;
SELECT [bracket_14403] FROM [dbo].[tbl_3];
SELECT 14404 AS id, 'row_14404' AS label;
SELECT nested FROM t WHERE id IN (14405, 14406, 14407);
/* block header 14406 */
DELETE FROM bench_t_7 WHERE id = 7;
WITH cte_14408 AS (SELECT 14408 AS n) SELECT n FROM cte_14408;
SELECT [bracket_14409] FROM [dbo].[tbl_9];
SELECT `mysql_14410` FROM `tbl_10`;
SELECT [bracket_14411] FROM [dbo].[tbl_11];
SELECT 14412 AS id, 'row_14412' AS label;
INSERT INTO bench_t_77 (id, payload) VALUES (14413, 'v14413');
SELECT * FROM "quoted_14414" WHERE col = E'esc\'14414';
SELECT [bracket_14415] FROM [dbo].[tbl_15];
SELECT `mysql_14416` FROM `tbl_16`;
# hash comment 14417
/* block header 14418 */
SELECT * FROM "quoted_14419" WHERE col = E'esc\'14419';
UPDATE bench_t_20 SET payload = 14420 WHERE id = 20;
$dz$ dollar body 14421 ; semicolon inside $dz$
WITH cte_14422 AS (SELECT 14422 AS n) SELECT n FROM cte_14422;
SELECT 14423 AS id, 'row_14423' AS label;
DELETE FROM bench_t_24 WHERE id = 8;
# hash comment 14425
SELECT 14426 AS id, 'row_14426' AS label;
SELECT 14427 AS id, 'row_14427' AS label;
SELECT [bracket_14428] FROM [dbo].[tbl_28];
-- line 14429: deterministic comment
SELECT `mysql_14430` FROM `tbl_30`;
# hash comment 14431
$dz$ dollar body 14432 ; semicolon inside $dz$
DELETE FROM bench_t_1 WHERE id = 1;
INSERT INTO bench_t_98 (id, payload) VALUES (14434, 'v14434');
# hash comment 14435
DELETE FROM bench_t_4 WHERE id = 4;
SELECT * FROM "quoted_14437" WHERE col = E'esc\'14437';
# hash comment 14438
BEGIN; SELECT 14439; COMMIT;
SELECT [bracket_14440] FROM [dbo].[tbl_0];
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
DELETE FROM bench_t_10 WHERE id = 10;
SELECT 14443 AS id, 'row_14443' AS label;
BEGIN; SELECT 14444; COMMIT;
SELECT `mysql_14445` FROM `tbl_45`;
-- line 14446: deterministic comment
UPDATE bench_t_47 SET payload = 14447 WHERE id = 15;
# hash comment 14448
SELECT * FROM "quoted_14449" WHERE col = E'esc\'14449';
/* block header 14450 */
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT [bracket_14452] FROM [dbo].[tbl_12];
BEGIN; SELECT 14453; COMMIT;
/* block header 14454 */
SELECT `mysql_14455` FROM `tbl_5`;
WITH cte_14456 AS (SELECT 14456 AS n) SELECT n FROM cte_14456;
/* block header 14457 */
/* block header 14458 */
WITH cte_14459 AS (SELECT 14459 AS n) SELECT n FROM cte_14459;
/* block header 14460 */
UPDATE bench_t_61 SET payload = 14461 WHERE id = 29;
WITH cte_14462 AS (SELECT 14462 AS n) SELECT n FROM cte_14462;
SELECT [bracket_14463] FROM [dbo].[tbl_23];
-- line 14464: deterministic comment
DELETE FROM bench_t_1 WHERE id = 1;
DELETE FROM bench_t_2 WHERE id = 2;
$dz$ dollar body 14467 ; semicolon inside $dz$
SELECT nested FROM t WHERE id IN (14468, 14469, 14470);
SELECT [bracket_14469] FROM [dbo].[tbl_29];
/* block header 14470 */
BEGIN; SELECT 14471; COMMIT;
# hash comment 14472
SELECT * FROM "quoted_14473" WHERE col = E'esc\'14473';
/* block header 14474 */
SELECT [bracket_14475] FROM [dbo].[tbl_35];
BEGIN; SELECT 14476; COMMIT;
SELECT 14477 AS id, 'row_14477' AS label;
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT [bracket_14479] FROM [dbo].[tbl_39];
SELECT * FROM "quoted_14480" WHERE col = E'esc\'14480';
SELECT [bracket_14481] FROM [dbo].[tbl_1];
DELETE FROM bench_t_18 WHERE id = 2;
$dz$ dollar body 14483 ; semicolon inside $dz$
SELECT * FROM "quoted_14484" WHERE col = E'esc\'14484';
UPDATE bench_t_21 SET payload = 14485 WHERE id = 21;
DELETE FROM bench_t_22 WHERE id = 6;
WITH cte_14487 AS (SELECT 14487 AS n) SELECT n FROM cte_14487;
UPDATE bench_t_24 SET payload = 14488 WHERE id = 24;
WITH cte_14489 AS (SELECT 14489 AS n) SELECT n FROM cte_14489;
SELECT [bracket_14490] FROM [dbo].[tbl_10];
-- line 14491: deterministic comment
WITH cte_14492 AS (SELECT 14492 AS n) SELECT n FROM cte_14492;
SELECT [bracket_14493] FROM [dbo].[tbl_13];
SELECT 14494 AS id, 'row_14494' AS label;
-- line 14495: deterministic comment
$dz$ dollar body 14496 ; semicolon inside $dz$
SELECT `mysql_14497` FROM `tbl_47`;
/* block header 14498 */
SELECT `mysql_14499` FROM `tbl_49`;
/*
 * section 58
 * checksum f98f
 */
/* block header 14500 */
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT 14506 AS id, 'row_14506' AS label;
SELECT * FROM "quoted_14507" WHERE col = E'esc\'14507';
SELECT 14508 AS id, 'row_14508' AS label;
-- line 14509: deterministic comment
BEGIN; SELECT 14510; COMMIT;
/* block header 14511 */
SELECT [bracket_14512] FROM [dbo].[tbl_32];
# hash comment 14513
SELECT 14514 AS id, 'row_14514' AS label;
/* block header 14515 */
SELECT [bracket_14516] FROM [dbo].[tbl_36];
SELECT * FROM "quoted_14517" WHERE col = E'esc\'14517';
WITH cte_14518 AS (SELECT 14518 AS n) SELECT n FROM cte_14518;
/* block header 14519 */
BEGIN; SELECT 14520; COMMIT;
BEGIN; SELECT 14521; COMMIT;
SELECT [bracket_14522] FROM [dbo].[tbl_2];
WITH cte_14523 AS (SELECT 14523 AS n) SELECT n FROM cte_14523;
SELECT `mysql_14524` FROM `tbl_24`;
INSERT INTO bench_t_61 (id, payload) VALUES (14525, 'v14525');
/* block header 14526 */
SELECT `mysql_14527` FROM `tbl_27`;
BEGIN; SELECT 14528; COMMIT;
DELETE FROM bench_t_1 WHERE id = 1;
SELECT [bracket_14530] FROM [dbo].[tbl_10];
SELECT `mysql_14531` FROM `tbl_31`;
WITH cte_14532 AS (SELECT 14532 AS n) SELECT n FROM cte_14532;
/* block header 14533 */
SELECT `mysql_14534` FROM `tbl_34`;
SELECT 14535 AS id, 'row_14535' AS label;
SELECT `mysql_14536` FROM `tbl_36`;
# hash comment 14537
# hash comment 14538
/* block header 14539 */
SELECT nested FROM t WHERE id IN (14540, 14541, 14542);
-- line 14541: deterministic comment
INSERT INTO bench_t_78 (id, payload) VALUES (14542, 'O''Brien');
# hash comment 14543
/* block header 14544 */
SELECT nested FROM t WHERE id IN (14545, 14546, 14547);
SELECT nested FROM t WHERE id IN (14546, 14547, 14548);
SELECT [bracket_14547] FROM [dbo].[tbl_27];
$dz$ dollar body 14548 ; semicolon inside $dz$
$dz$ dollar body 14549 ; semicolon inside $dz$
BEGIN; SELECT 14550; COMMIT;
BEGIN; SELECT 14551; COMMIT;
SELECT * FROM "quoted_14552" WHERE col = E'esc\'14552';
SELECT `mysql_14553` FROM `tbl_3`;
# hash comment 14554
$dz$ dollar body 14555 ; semicolon inside $dz$
SELECT 14556 AS id, 'row_14556' AS label;
-- line 14557: deterministic comment
SELECT `mysql_14558` FROM `tbl_8`;
/* block header 14559 */
SELECT * FROM "quoted_14560" WHERE col = E'esc\'14560';
SELECT [bracket_14561] FROM [dbo].[tbl_1];
DELETE FROM bench_t_2 WHERE id = 2;
-- line 14563: deterministic comment
# hash comment 14564
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
INSERT INTO bench_t_102 (id, payload) VALUES (14566, 'v14566');
UPDATE bench_t_39 SET payload = 14567 WHERE id = 7;
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
WITH cte_14569 AS (SELECT 14569 AS n) SELECT n FROM cte_14569;
SELECT [bracket_14570] FROM [dbo].[tbl_10];
BEGIN; SELECT 14571; COMMIT;
INSERT INTO bench_t_108 (id, payload) VALUES (14572, 'v14572');
SELECT `mysql_14573` FROM `tbl_23`;
DELETE FROM bench_t_14 WHERE id = 14;
INSERT INTO bench_t_111 (id, payload) VALUES (14575, 'O''Brien');
WITH cte_14576 AS (SELECT 14576 AS n) SELECT n FROM cte_14576;
SELECT nested FROM t WHERE id IN (14577, 14578, 14579);
INSERT INTO bench_t_114 (id, payload) VALUES (14578, 'v14578');
SELECT [bracket_14579] FROM [dbo].[tbl_19];
# hash comment 14580
WITH cte_14581 AS (SELECT 14581 AS n) SELECT n FROM cte_14581;
SELECT 14582 AS id, 'row_14582' AS label;
SELECT [bracket_14583] FROM [dbo].[tbl_23];
UPDATE bench_t_56 SET payload = 14584 WHERE id = 24;
INSERT INTO bench_t_121 (id, payload) VALUES (14585, 'v14585');
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT [bracket_14587] FROM [dbo].[tbl_27];
UPDATE bench_t_60 SET payload = 14588 WHERE id = 28;
-- line 14589: deterministic comment
INSERT INTO bench_t_126 (id, payload) VALUES (14590, 'v14590');
SELECT * FROM "quoted_14591" WHERE col = E'esc\'14591';
INSERT INTO bench_t_0 (id, payload) VALUES (14592, 'v14592');
SELECT 14593 AS id, 'row_14593' AS label;
UPDATE bench_t_2 SET payload = 14594 WHERE id = 2;
-- line 14595: deterministic comment
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
WITH cte_14598 AS (SELECT 14598 AS n) SELECT n FROM cte_14598;
INSERT INTO bench_t_7 (id, payload) VALUES (14599, 'v14599');
DELETE FROM bench_t_8 WHERE id = 8;
UPDATE bench_t_9 SET payload = 14601 WHERE id = 9;
UPDATE bench_t_10 SET payload = 14602 WHERE id = 10;
$dz$ dollar body 14603 ; semicolon inside $dz$
# hash comment 14604
WITH cte_14605 AS (SELECT 14605 AS n) SELECT n FROM cte_14605;
SELECT `mysql_14606` FROM `tbl_6`;
/* block header 14607 */
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
/* block header 14609 */
$dz$ dollar body 14610 ; semicolon inside $dz$
DELETE FROM bench_t_19 WHERE id = 3;
/* block header 14612 */
SELECT 14613 AS id, 'row_14613' AS label;
SELECT 14614 AS id, 'row_14614' AS label;
SELECT * FROM "quoted_14615" WHERE col = E'esc\'14615';
INSERT INTO bench_t_24 (id, payload) VALUES (14616, 'v14616');
SELECT * FROM "quoted_14617" WHERE col = E'esc\'14617';
$dz$ dollar body 14618 ; semicolon inside $dz$
/* block header 14619 */
DELETE FROM bench_t_28 WHERE id = 12;
DELETE FROM bench_t_29 WHERE id = 13;
WITH cte_14622 AS (SELECT 14622 AS n) SELECT n FROM cte_14622;
UPDATE bench_t_31 SET payload = 14623 WHERE id = 31;
SELECT `mysql_14624` FROM `tbl_24`;
$dz$ dollar body 14625 ; semicolon inside $dz$
/* block header 14626 */
BEGIN; SELECT 14627; COMMIT;
/* block header 14628 */
$dz$ dollar body 14629 ; semicolon inside $dz$
$dz$ dollar body 14630 ; semicolon inside $dz$
SELECT nested FROM t WHERE id IN (14631, 14632, 14633);
# hash comment 14632
SELECT * FROM "quoted_14633" WHERE col = E'esc\'14633';
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
# hash comment 14635
-- line 14636: deterministic comment
SELECT 14637 AS id, 'row_14637' AS label;
$dz$ dollar body 14638 ; semicolon inside $dz$
DELETE FROM bench_t_15 WHERE id = 15;
SELECT * FROM "quoted_14640" WHERE col = E'esc\'14640';
UPDATE bench_t_49 SET payload = 14641 WHERE id = 17;
BEGIN; SELECT 14642; COMMIT;
SELECT nested FROM t WHERE id IN (14643, 14644, 14645);
SELECT * FROM "quoted_14644" WHERE col = E'esc\'14644';
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
WITH cte_14646 AS (SELECT 14646 AS n) SELECT n FROM cte_14646;
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
/* block header 14648 */
SELECT 14649 AS id, 'row_14649' AS label;
DELETE FROM bench_t_26 WHERE id = 10;
# hash comment 14651
-- line 14652: deterministic comment
INSERT INTO bench_t_61 (id, payload) VALUES (14653, 'v14653');
INSERT INTO bench_t_62 (id, payload) VALUES (14654, 'v14654');
SELECT nested FROM t WHERE id IN (14655, 14656, 14657);
BEGIN; SELECT 14656; COMMIT;
SELECT 14657 AS id, 'row_14657' AS label;
INSERT INTO bench_t_66 (id, payload) VALUES (14658, 'v14658');
SELECT `mysql_14659` FROM `tbl_9`;
SELECT [bracket_14660] FROM [dbo].[tbl_20];
SELECT * FROM "quoted_14661" WHERE col = E'esc\'14661';
-- line 14662: deterministic comment
INSERT INTO bench_t_71 (id, payload) VALUES (14663, 'O''Brien');
SELECT nested FROM t WHERE id IN (14664, 14665, 14666);
# hash comment 14665
WITH cte_14666 AS (SELECT 14666 AS n) SELECT n FROM cte_14666;
DELETE FROM bench_t_11 WHERE id = 11;
UPDATE bench_t_12 SET payload = 14668 WHERE id = 12;
-- line 14669: deterministic comment
# hash comment 14670
# hash comment 14671
SELECT `mysql_14672` FROM `tbl_22`;
/* block header 14673 */
UPDATE bench_t_18 SET payload = 14674 WHERE id = 18;
DELETE FROM bench_t_19 WHERE id = 3;
SELECT `mysql_14676` FROM `tbl_26`;
DELETE FROM bench_t_21 WHERE id = 5;
SELECT nested FROM t WHERE id IN (14678, 14679, 14680);
SELECT `mysql_14679` FROM `tbl_29`;
-- line 14680: deterministic comment
BEGIN; SELECT 14681; COMMIT;
WITH cte_14682 AS (SELECT 14682 AS n) SELECT n FROM cte_14682;
BEGIN; SELECT 14683; COMMIT;
INSERT INTO bench_t_92 (id, payload) VALUES (14684, 'v14684');
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT 14686 AS id, 'row_14686' AS label;
WITH cte_14687 AS (SELECT 14687 AS n) SELECT n FROM cte_14687;
-- line 14688: deterministic comment
-- line 14689: deterministic comment
SELECT nested FROM t WHERE id IN (14690, 14691, 14692);
SELECT nested FROM t WHERE id IN (14691, 14692, 14693);
DELETE FROM bench_t_4 WHERE id = 4;
$dz$ dollar body 14693 ; semicolon inside $dz$
WITH cte_14694 AS (SELECT 14694 AS n) SELECT n FROM cte_14694;
SELECT [bracket_14695] FROM [dbo].[tbl_15];
SELECT * FROM "quoted_14696" WHERE col = E'esc\'14696';
BEGIN; SELECT 14697; COMMIT;
UPDATE bench_t_42 SET payload = 14698 WHERE id = 10;
# hash comment 14699
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT 14701 AS id, 'row_14701' AS label;
/* block header 14702 */
SELECT nested FROM t WHERE id IN (14703, 14704, 14705);
SELECT `mysql_14704` FROM `tbl_4`;
SELECT 14705 AS id, 'row_14705' AS label;
-- line 14706: deterministic comment
SELECT `mysql_14707` FROM `tbl_7`;
SELECT nested FROM t WHERE id IN (14708, 14709, 14710);
# hash comment 14709
SELECT * FROM "quoted_14710" WHERE col = E'esc\'14710';
SELECT 14711 AS id, 'row_14711' AS label;
SELECT 14712 AS id, 'row_14712' AS label;
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT 14714 AS id, 'row_14714' AS label;
BEGIN; SELECT 14715; COMMIT;
SELECT * FROM "quoted_14716" WHERE col = E'esc\'14716';
SELECT * FROM "quoted_14717" WHERE col = E'esc\'14717';
$dz$ dollar body 14718 ; semicolon inside $dz$
/* block header 14719 */
$dz$ dollar body 14720 ; semicolon inside $dz$
SELECT nested FROM t WHERE id IN (14721, 14722, 14723);
BEGIN; SELECT 14722; COMMIT;
SELECT nested FROM t WHERE id IN (14723, 14724, 14725);
-- line 14724: deterministic comment
SELECT `mysql_14725` FROM `tbl_25`;
$dz$ dollar body 14726 ; semicolon inside $dz$
/* block header 14727 */
UPDATE bench_t_8 SET payload = 14728 WHERE id = 8;
SELECT 14729 AS id, 'row_14729' AS label;
WITH cte_14730 AS (SELECT 14730 AS n) SELECT n FROM cte_14730;
# hash comment 14731
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
-- line 14733: deterministic comment
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT nested FROM t WHERE id IN (14735, 14736, 14737);
DELETE FROM bench_t_16 WHERE id = 0;
$dz$ dollar body 14737 ; semicolon inside $dz$
WITH cte_14738 AS (SELECT 14738 AS n) SELECT n FROM cte_14738;
SELECT * FROM "quoted_14739" WHERE col = E'esc\'14739';
UPDATE bench_t_20 SET payload = 14740 WHERE id = 20;
SELECT 14741 AS id, 'row_14741' AS label;
DELETE FROM bench_t_22 WHERE id = 6;
-- line 14743: deterministic comment
SELECT [bracket_14744] FROM [dbo].[tbl_24];
BEGIN; SELECT 14745; COMMIT;
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
INSERT INTO bench_t_27 (id, payload) VALUES (14747, 'v14747');
SELECT [bracket_14748] FROM [dbo].[tbl_28];
SELECT [bracket_14749] FROM [dbo].[tbl_29];
/*
 * section 59
 * checksum 4d4b
 */
INSERT INTO bench_t_30 (id, payload) VALUES (14750, 'v14750');
SELECT * FROM "quoted_14755" WHERE col = E'esc\'14755';
SELECT nested FROM t WHERE id IN (14756, 14757, 14758);
SELECT * FROM "quoted_14757" WHERE col = E'esc\'14757';
SELECT [bracket_14758] FROM [dbo].[tbl_38];
WITH cte_14759 AS (SELECT 14759 AS n) SELECT n FROM cte_14759;
WITH cte_14760 AS (SELECT 14760 AS n) SELECT n FROM cte_14760;
SELECT [bracket_14761] FROM [dbo].[tbl_1];
BEGIN; SELECT 14762; COMMIT;
WITH cte_14763 AS (SELECT 14763 AS n) SELECT n FROM cte_14763;
SELECT `mysql_14764` FROM `tbl_14`;
# hash comment 14765
-- line 14766: deterministic comment
BEGIN; SELECT 14767; COMMIT;
SELECT * FROM "quoted_14768" WHERE col = E'esc\'14768';
$dz$ dollar body 14769 ; semicolon inside $dz$
SELECT `mysql_14770` FROM `tbl_20`;
# hash comment 14771
WITH cte_14772 AS (SELECT 14772 AS n) SELECT n FROM cte_14772;
/* block header 14773 */
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
-- line 14775: deterministic comment
INSERT INTO bench_t_56 (id, payload) VALUES (14776, 'v14776');
SELECT `mysql_14777` FROM `tbl_27`;
/* block header 14778 */
SELECT nested FROM t WHERE id IN (14779, 14780, 14781);
# hash comment 14780
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
-- line 14782: deterministic comment
WITH cte_14783 AS (SELECT 14783 AS n) SELECT n FROM cte_14783;
WITH cte_14784 AS (SELECT 14784 AS n) SELECT n FROM cte_14784;
INSERT INTO bench_t_65 (id, payload) VALUES (14785, 'v14785');
WITH cte_14786 AS (SELECT 14786 AS n) SELECT n FROM cte_14786;
WITH cte_14787 AS (SELECT 14787 AS n) SELECT n FROM cte_14787;
INSERT INTO bench_t_68 (id, payload) VALUES (14788, 'v14788');
SELECT `mysql_14789` FROM `tbl_39`;
SELECT `mysql_14790` FROM `tbl_40`;
SELECT `mysql_14791` FROM `tbl_41`;
$dz$ dollar body 14792 ; semicolon inside $dz$
$dz$ dollar body 14793 ; semicolon inside $dz$
SELECT `mysql_14794` FROM `tbl_44`;
# hash comment 14795
SELECT 14796 AS id, 'row_14796' AS label;
DELETE FROM bench_t_13 WHERE id = 13;
# hash comment 14798
$dz$ dollar body 14799 ; semicolon inside $dz$
-- line 14800: deterministic comment
-- line 14801: deterministic comment
SELECT [bracket_14802] FROM [dbo].[tbl_2];
UPDATE bench_t_19 SET payload = 14803 WHERE id = 19;
SELECT [bracket_14804] FROM [dbo].[tbl_4];
$dz$ dollar body 14805 ; semicolon inside $dz$
$dz$ dollar body 14806 ; semicolon inside $dz$
SELECT * FROM "quoted_14807" WHERE col = E'esc\'14807';
UPDATE bench_t_24 SET payload = 14808 WHERE id = 24;
-- line 14809: deterministic comment
INSERT INTO bench_t_90 (id, payload) VALUES (14810, 'v14810');
WITH cte_14811 AS (SELECT 14811 AS n) SELECT n FROM cte_14811;
$dz$ dollar body 14812 ; semicolon inside $dz$
# hash comment 14813
SELECT [bracket_14814] FROM [dbo].[tbl_14];
SELECT `mysql_14815` FROM `tbl_15`;
SELECT * FROM "quoted_14816" WHERE col = E'esc\'14816';
-- line 14817: deterministic comment
SELECT nested FROM t WHERE id IN (14818, 14819, 14820);
UPDATE bench_t_35 SET payload = 14819 WHERE id = 3;
$dz$ dollar body 14820 ; semicolon inside $dz$
WITH cte_14821 AS (SELECT 14821 AS n) SELECT n FROM cte_14821;
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT [bracket_14823] FROM [dbo].[tbl_23];
INSERT INTO bench_t_104 (id, payload) VALUES (14824, 'v14824');
$dz$ dollar body 14825 ; semicolon inside $dz$
SELECT 14826 AS id, 'row_14826' AS label;
SELECT 14827 AS id, 'row_14827' AS label;
$dz$ dollar body 14828 ; semicolon inside $dz$
SELECT [bracket_14829] FROM [dbo].[tbl_29];
WITH cte_14830 AS (SELECT 14830 AS n) SELECT n FROM cte_14830;
DELETE FROM bench_t_15 WHERE id = 15;
# hash comment 14832
SELECT 14833 AS id, 'row_14833' AS label;
SELECT 14834 AS id, 'row_14834' AS label;
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT [bracket_14837] FROM [dbo].[tbl_37];
SELECT `mysql_14838` FROM `tbl_38`;
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT [bracket_14840] FROM [dbo].[tbl_0];
SELECT [bracket_14841] FROM [dbo].[tbl_1];
WITH cte_14842 AS (SELECT 14842 AS n) SELECT n FROM cte_14842;
SELECT [bracket_14843] FROM [dbo].[tbl_3];
INSERT INTO bench_t_124 (id, payload) VALUES (14844, 'v14844');
SELECT `mysql_14845` FROM `tbl_45`;
-- line 14846: deterministic comment
-- line 14847: deterministic comment
DELETE FROM bench_t_0 WHERE id = 0;
-- line 14849: deterministic comment
/* block header 14850 */
BEGIN; SELECT 14851; COMMIT;
-- line 14852: deterministic comment
SELECT nested FROM t WHERE id IN (14853, 14854, 14855);
# hash comment 14854
$dz$ dollar body 14855 ; semicolon inside $dz$
SELECT nested FROM t WHERE id IN (14856, 14857, 14858);
WITH cte_14857 AS (SELECT 14857 AS n) SELECT n FROM cte_14857;
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT 14859 AS id, 'row_14859' AS label;
# hash comment 14860
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
/* block header 14862 */
BEGIN; SELECT 14863; COMMIT;
WITH cte_14864 AS (SELECT 14864 AS n) SELECT n FROM cte_14864;
SELECT nested FROM t WHERE id IN (14865, 14866, 14867);
DELETE FROM bench_t_18 WHERE id = 2;
# hash comment 14867
INSERT INTO bench_t_20 (id, payload) VALUES (14868, 'v14868');
SELECT `mysql_14869` FROM `tbl_19`;
DELETE FROM bench_t_22 WHERE id = 6;
SELECT 14871 AS id, 'row_14871' AS label;
INSERT INTO bench_t_24 (id, payload) VALUES (14872, 'O''Brien');
DELETE FROM bench_t_25 WHERE id = 9;
$dz$ dollar body 14874 ; semicolon inside $dz$
SELECT `mysql_14875` FROM `tbl_25`;
SELECT * FROM "quoted_14876" WHERE col = E'esc\'14876';
SELECT 14877 AS id, 'row_14877' AS label;
$dz$ dollar body 14878 ; semicolon inside $dz$
SELECT [bracket_14879] FROM [dbo].[tbl_39];
UPDATE bench_t_32 SET payload = 14880 WHERE id = 0;
$dz$ dollar body 14881 ; semicolon inside $dz$
SELECT 14882 AS id, 'row_14882' AS label;
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT 14885 AS id, 'row_14885' AS label;
WITH cte_14886 AS (SELECT 14886 AS n) SELECT n FROM cte_14886;
$dz$ dollar body 14887 ; semicolon inside $dz$
WITH cte_14888 AS (SELECT 14888 AS n) SELECT n FROM cte_14888;
$dz$ dollar body 14889 ; semicolon inside $dz$
INSERT INTO bench_t_42 (id, payload) VALUES (14890, 'v14890');
DELETE FROM bench_t_11 WHERE id = 11;
-- line 14892: deterministic comment
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT `mysql_14894` FROM `tbl_44`;
# hash comment 14895
WITH cte_14896 AS (SELECT 14896 AS n) SELECT n FROM cte_14896;
$dz$ dollar body 14897 ; semicolon inside $dz$
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
INSERT INTO bench_t_52 (id, payload) VALUES (14900, 'v14900');
SELECT * FROM "quoted_14901" WHERE col = E'esc\'14901';
WITH cte_14902 AS (SELECT 14902 AS n) SELECT n FROM cte_14902;
DELETE FROM bench_t_23 WHERE id = 7;
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
/* block header 14905 */
# hash comment 14906
SELECT * FROM "quoted_14907" WHERE col = E'esc\'14907';
SELECT nested FROM t WHERE id IN (14908, 14909, 14910);
WITH cte_14909 AS (SELECT 14909 AS n) SELECT n FROM cte_14909;
DELETE FROM bench_t_30 WHERE id = 14;
BEGIN; SELECT 14911; COMMIT;
SELECT nested FROM t WHERE id IN (14912, 14913, 14914);
UPDATE bench_t_1 SET payload = 14913 WHERE id = 1;
SELECT [bracket_14914] FROM [dbo].[tbl_34];
SELECT `mysql_14915` FROM `tbl_15`;
SELECT `mysql_14916` FROM `tbl_16`;
SELECT [bracket_14917] FROM [dbo].[tbl_37];
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
INSERT INTO bench_t_71 (id, payload) VALUES (14919, 'v14919');
UPDATE bench_t_8 SET payload = 14920 WHERE id = 8;
$dz$ dollar body 14921 ; semicolon inside $dz$
# hash comment 14922
SELECT nested FROM t WHERE id IN (14923, 14924, 14925);
WITH cte_14924 AS (SELECT 14924 AS n) SELECT n FROM cte_14924;
DELETE FROM bench_t_13 WHERE id = 13;
SELECT 14926 AS id, 'row_14926' AS label;
WITH cte_14927 AS (SELECT 14927 AS n) SELECT n FROM cte_14927;
# hash comment 14928
SELECT [bracket_14929] FROM [dbo].[tbl_9];
SELECT 14930 AS id, 'row_14930' AS label;
SELECT `mysql_14931` FROM `tbl_31`;
UPDATE bench_t_20 SET payload = 14932 WHERE id = 20;
$dz$ dollar body 14933 ; semicolon inside $dz$
UPDATE bench_t_22 SET payload = 14934 WHERE id = 22;
SELECT [bracket_14935] FROM [dbo].[tbl_15];
DELETE FROM bench_t_24 WHERE id = 8;
INSERT INTO bench_t_89 (id, payload) VALUES (14937, 'v14937');
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT * FROM "quoted_14939" WHERE col = E'esc\'14939';
-- line 14940: deterministic comment
SELECT 14941 AS id, 'row_14941' AS label;
SELECT * FROM "quoted_14942" WHERE col = E'esc\'14942';
UPDATE bench_t_31 SET payload = 14943 WHERE id = 31;
INSERT INTO bench_t_96 (id, payload) VALUES (14944, 'v14944');
SELECT `mysql_14945` FROM `tbl_45`;
SELECT `mysql_14946` FROM `tbl_46`;
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
/* block header 14948 */
-- line 14949: deterministic comment
# hash comment 14950
SELECT nested FROM t WHERE id IN (14951, 14952, 14953);
BEGIN; SELECT 14952; COMMIT;
SELECT `mysql_14953` FROM `tbl_3`;
# hash comment 14954
SELECT [bracket_14955] FROM [dbo].[tbl_35];
# hash comment 14956
BEGIN; SELECT 14957; COMMIT;
INSERT INTO bench_t_110 (id, payload) VALUES (14958, 'v14958');
WITH cte_14959 AS (SELECT 14959 AS n) SELECT n FROM cte_14959;
DELETE FROM bench_t_16 WHERE id = 0;
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
-- line 14962: deterministic comment
SELECT [bracket_14963] FROM [dbo].[tbl_3];
UPDATE bench_t_52 SET payload = 14964 WHERE id = 20;
SELECT `mysql_14965` FROM `tbl_15`;
SELECT `mysql_14966` FROM `tbl_16`;
DELETE FROM bench_t_23 WHERE id = 7;
# hash comment 14968
DELETE FROM bench_t_25 WHERE id = 9;
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT nested FROM t WHERE id IN (14971, 14972, 14973);
INSERT INTO bench_t_124 (id, payload) VALUES (14972, 'v14972');
BEGIN; SELECT 14973; COMMIT;
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
UPDATE bench_t_63 SET payload = 14975 WHERE id = 31;
SELECT 14976 AS id, 'row_14976' AS label;
SELECT [bracket_14977] FROM [dbo].[tbl_17];
SELECT 14978 AS id, 'row_14978' AS label;
WITH cte_14979 AS (SELECT 14979 AS n) SELECT n FROM cte_14979;
SELECT 14980 AS id, 'row_14980' AS label;
INSERT INTO bench_t_5 (id, payload) VALUES (14981, 'v14981');
INSERT INTO bench_t_6 (id, payload) VALUES (14982, 'O''Brien');
INSERT INTO bench_t_7 (id, payload) VALUES (14983, 'v14983');
SELECT * FROM "quoted_14984" WHERE col = E'esc\'14984';
SELECT * FROM "quoted_14985" WHERE col = E'esc\'14985';
SELECT nested FROM t WHERE id IN (14986, 14987, 14988);
DELETE FROM bench_t_11 WHERE id = 11;
DELETE FROM bench_t_12 WHERE id = 12;
SELECT nested FROM t WHERE id IN (14989, 14990, 14991);
SELECT 14990 AS id, 'row_14990' AS label;
UPDATE bench_t_15 SET payload = 14991 WHERE id = 15;
SELECT `mysql_14992` FROM `tbl_42`;
SELECT [bracket_14993] FROM [dbo].[tbl_33];
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
/* block header 14995 */
SELECT * FROM "quoted_14996" WHERE col = E'esc\'14996';
# hash comment 14997
/* block header 14998 */
SELECT [bracket_14999] FROM [dbo].[tbl_39];
/*
 * section 60
 * checksum deff
 */
SELECT * FROM "quoted_15000" WHERE col = E'esc\'15000';
WITH cte_15005 AS (SELECT 15005 AS n) SELECT n FROM cte_15005;
INSERT INTO bench_t_30 (id, payload) VALUES (15006, 'v15006');
SELECT * FROM "quoted_15007" WHERE col = E'esc\'15007';
$dz$ dollar body 15008 ; semicolon inside $dz$
DELETE FROM bench_t_1 WHERE id = 1;
WITH cte_15010 AS (SELECT 15010 AS n) SELECT n FROM cte_15010;
$dz$ dollar body 15011 ; semicolon inside $dz$
/* block header 15012 */
# hash comment 15013
SELECT nested FROM t WHERE id IN (15014, 15015, 15016);
BEGIN; SELECT 15015; COMMIT;
DELETE FROM bench_t_8 WHERE id = 8;
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT [bracket_15018] FROM [dbo].[tbl_18];
-- line 15019: deterministic comment
UPDATE bench_t_44 SET payload = 15020 WHERE id = 12;
WITH cte_15021 AS (SELECT 15021 AS n) SELECT n FROM cte_15021;
WITH cte_15022 AS (SELECT 15022 AS n) SELECT n FROM cte_15022;
SELECT 15023 AS id, 'row_15023' AS label;
-- line 15024: deterministic comment
SELECT nested FROM t WHERE id IN (15025, 15026, 15027);
SELECT * FROM "quoted_15026" WHERE col = E'esc\'15026';
SELECT [bracket_15027] FROM [dbo].[tbl_27];
INSERT INTO bench_t_52 (id, payload) VALUES (15028, 'v15028');
SELECT `mysql_15029` FROM `tbl_29`;
/* block header 15030 */
UPDATE bench_t_55 SET payload = 15031 WHERE id = 23;
SELECT [bracket_15032] FROM [dbo].[tbl_32];
SELECT `mysql_15033` FROM `tbl_33`;
SELECT [bracket_15034] FROM [dbo].[tbl_34];
/* block header 15035 */
SELECT * FROM "quoted_15036" WHERE col = E'esc\'15036';
INSERT INTO bench_t_61 (id, payload) VALUES (15037, 'O''Brien');
SELECT nested FROM t WHERE id IN (15038, 15039, 15040);
WITH cte_15039 AS (SELECT 15039 AS n) SELECT n FROM cte_15039;
/* block header 15040 */
SELECT 15041 AS id, 'row_15041' AS label;
-- line 15042: deterministic comment
BEGIN; SELECT 15043; COMMIT;
$dz$ dollar body 15044 ; semicolon inside $dz$
INSERT INTO bench_t_69 (id, payload) VALUES (15045, 'v15045');
BEGIN; SELECT 15046; COMMIT;
$dz$ dollar body 15047 ; semicolon inside $dz$
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
$dz$ dollar body 15049 ; semicolon inside $dz$
/* block header 15050 */
# hash comment 15051
SELECT * FROM "quoted_15052" WHERE col = E'esc\'15052';
UPDATE bench_t_13 SET payload = 15053 WHERE id = 13;
SELECT nested FROM t WHERE id IN (15054, 15055, 15056);
WITH cte_15055 AS (SELECT 15055 AS n) SELECT n FROM cte_15055;
INSERT INTO bench_t_80 (id, payload) VALUES (15056, 'v15056');
# hash comment 15057
DELETE FROM bench_t_18 WHERE id = 2;
BEGIN; SELECT 15059; COMMIT;
SELECT nested FROM t WHERE id IN (15060, 15061, 15062);
$dz$ dollar body 15061 ; semicolon inside $dz$
BEGIN; SELECT 15062; COMMIT;
WITH cte_15063 AS (SELECT 15063 AS n) SELECT n FROM cte_15063;
-- line 15064: deterministic comment
/* block header 15065 */
SELECT nested FROM t WHERE id IN (15066, 15067, 15068);
SELECT * FROM "quoted_15067" WHERE col = E'esc\'15067';
SELECT * FROM "quoted_15068" WHERE col = E'esc\'15068';
SELECT [bracket_15069] FROM [dbo].[tbl_29];
$dz$ dollar body 15070 ; semicolon inside $dz$
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
$dz$ dollar body 15072 ; semicolon inside $dz$
# hash comment 15073
BEGIN; SELECT 15074; COMMIT;
INSERT INTO bench_t_99 (id, payload) VALUES (15075, 'v15075');
SELECT `mysql_15076` FROM `tbl_26`;
/* block header 15077 */
# hash comment 15078
SELECT * FROM "quoted_15079" WHERE col = E'esc\'15079';
SELECT 15080 AS id, 'row_15080' AS label;
SELECT `mysql_15081` FROM `tbl_31`;
SELECT 15082 AS id, 'row_15082' AS label;
INSERT INTO bench_t_107 (id, payload) VALUES (15083, 'v15083');
SELECT [bracket_15084] FROM [dbo].[tbl_4];
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT * FROM "quoted_15086" WHERE col = E'esc\'15086';
SELECT `mysql_15087` FROM `tbl_37`;
BEGIN; SELECT 15088; COMMIT;
BEGIN; SELECT 15089; COMMIT;
WITH cte_15090 AS (SELECT 15090 AS n) SELECT n FROM cte_15090;
INSERT INTO bench_t_115 (id, payload) VALUES (15091, 'v15091');
SELECT nested FROM t WHERE id IN (15092, 15093, 15094);
BEGIN; SELECT 15093; COMMIT;
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
BEGIN; SELECT 15095; COMMIT;
WITH cte_15096 AS (SELECT 15096 AS n) SELECT n FROM cte_15096;
DELETE FROM bench_t_25 WHERE id = 9;
SELECT nested FROM t WHERE id IN (15098, 15099, 15100);
SELECT * FROM "quoted_15099" WHERE col = E'esc\'15099';
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT nested FROM t WHERE id IN (15101, 15102, 15103);
UPDATE bench_t_62 SET payload = 15102 WHERE id = 30;
INSERT INTO bench_t_127 (id, payload) VALUES (15103, 'O''Brien');
SELECT `mysql_15104` FROM `tbl_4`;
SELECT * FROM "quoted_15105" WHERE col = E'esc\'15105';
SELECT [bracket_15106] FROM [dbo].[tbl_26];
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT * FROM "quoted_15108" WHERE col = E'esc\'15108';
WITH cte_15109 AS (SELECT 15109 AS n) SELECT n FROM cte_15109;
INSERT INTO bench_t_6 (id, payload) VALUES (15110, 'v15110');
-- line 15111: deterministic comment
UPDATE bench_t_8 SET payload = 15112 WHERE id = 8;
SELECT `mysql_15113` FROM `tbl_13`;
SELECT * FROM "quoted_15114" WHERE col = E'esc\'15114';
SELECT * FROM "quoted_15115" WHERE col = E'esc\'15115';
SELECT 15116 AS id, 'row_15116' AS label;
WITH cte_15117 AS (SELECT 15117 AS n) SELECT n FROM cte_15117;
BEGIN; SELECT 15118; COMMIT;
# hash comment 15119
$dz$ dollar body 15120 ; semicolon inside $dz$
UPDATE bench_t_17 SET payload = 15121 WHERE id = 17;
$dz$ dollar body 15122 ; semicolon inside $dz$
BEGIN; SELECT 15123; COMMIT;
SELECT `mysql_15124` FROM `tbl_24`;
UPDATE bench_t_21 SET payload = 15125 WHERE id = 21;
UPDATE bench_t_22 SET payload = 15126 WHERE id = 22;
SELECT * FROM "quoted_15127" WHERE col = E'esc\'15127';
SELECT * FROM "quoted_15128" WHERE col = E'esc\'15128';
$dz$ dollar body 15129 ; semicolon inside $dz$
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
$dz$ dollar body 15131 ; semicolon inside $dz$
WITH cte_15132 AS (SELECT 15132 AS n) SELECT n FROM cte_15132;
# hash comment 15133
SELECT nested FROM t WHERE id IN (15134, 15135, 15136);
SELECT `mysql_15135` FROM `tbl_35`;
DELETE FROM bench_t_0 WHERE id = 0;
$dz$ dollar body 15137 ; semicolon inside $dz$
DELETE FROM bench_t_2 WHERE id = 2;
SELECT nested FROM t WHERE id IN (15139, 15140, 15141);
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
BEGIN; SELECT 15141; COMMIT;
SELECT * FROM "quoted_15142" WHERE col = E'esc\'15142';
UPDATE bench_t_39 SET payload = 15143 WHERE id = 7;
SELECT [bracket_15144] FROM [dbo].[tbl_24];
/* block header 15145 */
SELECT 15146 AS id, 'row_15146' AS label;
/* block header 15147 */
SELECT nested FROM t WHERE id IN (15148, 15149, 15150);
DELETE FROM bench_t_13 WHERE id = 13;
UPDATE bench_t_46 SET payload = 15150 WHERE id = 14;
WITH cte_15151 AS (SELECT 15151 AS n) SELECT n FROM cte_15151;
/* block header 15152 */
SELECT nested FROM t WHERE id IN (15153, 15154, 15155);
SELECT [bracket_15154] FROM [dbo].[tbl_34];
WITH cte_15155 AS (SELECT 15155 AS n) SELECT n FROM cte_15155;
-- line 15156: deterministic comment
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
BEGIN; SELECT 15158; COMMIT;
SELECT 15159 AS id, 'row_15159' AS label;
UPDATE bench_t_56 SET payload = 15160 WHERE id = 24;
-- line 15161: deterministic comment
WITH cte_15162 AS (SELECT 15162 AS n) SELECT n FROM cte_15162;
SELECT * FROM "quoted_15163" WHERE col = E'esc\'15163';
/* block header 15164 */
# hash comment 15165
SELECT `mysql_15166` FROM `tbl_16`;
# hash comment 15167
UPDATE bench_t_0 SET payload = 15168 WHERE id = 0;
/* block header 15169 */
UPDATE bench_t_2 SET payload = 15170 WHERE id = 2;
-- line 15171: deterministic comment
INSERT INTO bench_t_68 (id, payload) VALUES (15172, 'v15172');
SELECT `mysql_15173` FROM `tbl_23`;
SELECT `mysql_15174` FROM `tbl_24`;
SELECT `mysql_15175` FROM `tbl_25`;
UPDATE bench_t_8 SET payload = 15176 WHERE id = 8;
SELECT [bracket_15177] FROM [dbo].[tbl_17];
SELECT nested FROM t WHERE id IN (15178, 15179, 15180);
SELECT [bracket_15179] FROM [dbo].[tbl_19];
DELETE FROM bench_t_12 WHERE id = 12;
SELECT nested FROM t WHERE id IN (15181, 15182, 15183);
SELECT * FROM "quoted_15182" WHERE col = E'esc\'15182';
SELECT 15183 AS id, 'row_15183' AS label;
SELECT `mysql_15184` FROM `tbl_34`;
WITH cte_15185 AS (SELECT 15185 AS n) SELECT n FROM cte_15185;
DELETE FROM bench_t_18 WHERE id = 2;
INSERT INTO bench_t_83 (id, payload) VALUES (15187, 'v15187');
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT [bracket_15189] FROM [dbo].[tbl_29];
WITH cte_15190 AS (SELECT 15190 AS n) SELECT n FROM cte_15190;
SELECT 15191 AS id, 'row_15191' AS label;
SELECT [bracket_15192] FROM [dbo].[tbl_32];
/* block header 15193 */
SELECT [bracket_15194] FROM [dbo].[tbl_34];
SELECT `mysql_15195` FROM `tbl_45`;
SELECT `mysql_15196` FROM `tbl_46`;
SELECT 15197 AS id, 'row_15197' AS label;
DELETE FROM bench_t_30 WHERE id = 14;
DELETE FROM bench_t_31 WHERE id = 15;
UPDATE bench_t_32 SET payload = 15200 WHERE id = 0;
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
WITH cte_15202 AS (SELECT 15202 AS n) SELECT n FROM cte_15202;
WITH cte_15203 AS (SELECT 15203 AS n) SELECT n FROM cte_15203;
UPDATE bench_t_36 SET payload = 15204 WHERE id = 4;
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT nested FROM t WHERE id IN (15206, 15207, 15208);
INSERT INTO bench_t_103 (id, payload) VALUES (15207, 'v15207');
INSERT INTO bench_t_104 (id, payload) VALUES (15208, 'v15208');
/* block header 15209 */
SELECT [bracket_15210] FROM [dbo].[tbl_10];
SELECT `mysql_15211` FROM `tbl_11`;
UPDATE bench_t_44 SET payload = 15212 WHERE id = 12;
SELECT nested FROM t WHERE id IN (15213, 15214, 15215);
SELECT * FROM "quoted_15214" WHERE col = E'esc\'15214';
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
/* block header 15216 */
SELECT [bracket_15217] FROM [dbo].[tbl_17];
/* block header 15218 */
UPDATE bench_t_51 SET payload = 15219 WHERE id = 19;
-- line 15220: deterministic comment
/* block header 15221 */
DELETE FROM bench_t_22 WHERE id = 6;
BEGIN; SELECT 15223; COMMIT;
SELECT nested FROM t WHERE id IN (15224, 15225, 15226);
# hash comment 15225
DELETE FROM bench_t_26 WHERE id = 10;
UPDATE bench_t_59 SET payload = 15227 WHERE id = 27;
$dz$ dollar body 15228 ; semicolon inside $dz$
SELECT * FROM "quoted_15229" WHERE col = E'esc\'15229';
BEGIN; SELECT 15230; COMMIT;
SELECT nested FROM t WHERE id IN (15231, 15232, 15233);
/* block header 15232 */
BEGIN; SELECT 15233; COMMIT;
/* block header 15234 */
SELECT `mysql_15235` FROM `tbl_35`;
$dz$ dollar body 15236 ; semicolon inside $dz$
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
-- line 15238: deterministic comment
SELECT `mysql_15239` FROM `tbl_39`;
DELETE FROM bench_t_8 WHERE id = 8;
SELECT nested FROM t WHERE id IN (15241, 15242, 15243);
WITH cte_15242 AS (SELECT 15242 AS n) SELECT n FROM cte_15242;
SELECT `mysql_15243` FROM `tbl_43`;
/* block header 15244 */
-- line 15245: deterministic comment
SELECT * FROM "quoted_15246" WHERE col = E'esc\'15246';
/* block header 15247 */
SELECT `mysql_15248` FROM `tbl_48`;
DELETE FROM bench_t_17 WHERE id = 1;
/*
 * section 61
 * checksum d10d
 */
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
INSERT INTO bench_t_23 (id, payload) VALUES (15255, 'v15255');
UPDATE bench_t_24 SET payload = 15256 WHERE id = 24;
/* block header 15257 */
SELECT [bracket_15258] FROM [dbo].[tbl_18];
SELECT 15259 AS id, 'row_15259' AS label;
SELECT [bracket_15260] FROM [dbo].[tbl_20];
SELECT nested FROM t WHERE id IN (15261, 15262, 15263);
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
BEGIN; SELECT 15263; COMMIT;
DELETE FROM bench_t_0 WHERE id = 0;
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
INSERT INTO bench_t_34 (id, payload) VALUES (15266, 'v15266');
UPDATE bench_t_35 SET payload = 15267 WHERE id = 3;
SELECT * FROM "quoted_15268" WHERE col = E'esc\'15268';
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT 15270 AS id, 'row_15270' AS label;
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
DELETE FROM bench_t_8 WHERE id = 8;
WITH cte_15273 AS (SELECT 15273 AS n) SELECT n FROM cte_15273;
SELECT `mysql_15274` FROM `tbl_24`;
SELECT nested FROM t WHERE id IN (15275, 15276, 15277);
/* block header 15276 */
WITH cte_15277 AS (SELECT 15277 AS n) SELECT n FROM cte_15277;
SELECT * FROM "quoted_15278" WHERE col = E'esc\'15278';
# hash comment 15279
INSERT INTO bench_t_48 (id, payload) VALUES (15280, 'v15280');
SELECT nested FROM t WHERE id IN (15281, 15282, 15283);
SELECT nested FROM t WHERE id IN (15282, 15283, 15284);
WITH cte_15283 AS (SELECT 15283 AS n) SELECT n FROM cte_15283;
-- line 15284: deterministic comment
SELECT 15285 AS id, 'row_15285' AS label;
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
WITH cte_15287 AS (SELECT 15287 AS n) SELECT n FROM cte_15287;
SELECT nested FROM t WHERE id IN (15288, 15289, 15290);
-- line 15289: deterministic comment
BEGIN; SELECT 15290; COMMIT;
DELETE FROM bench_t_27 WHERE id = 11;
/* block header 15292 */
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT * FROM "quoted_15294" WHERE col = E'esc\'15294';
DELETE FROM bench_t_31 WHERE id = 15;
/* block header 15296 */
SELECT `mysql_15297` FROM `tbl_47`;
SELECT nested FROM t WHERE id IN (15298, 15299, 15300);
BEGIN; SELECT 15299; COMMIT;
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT 15301 AS id, 'row_15301' AS label;
INSERT INTO bench_t_70 (id, payload) VALUES (15302, 'v15302');
UPDATE bench_t_7 SET payload = 15303 WHERE id = 7;
BEGIN; SELECT 15304; COMMIT;
SELECT * FROM "quoted_15305" WHERE col = E'esc\'15305';
# hash comment 15306
SELECT * FROM "quoted_15307" WHERE col = E'esc\'15307';
# hash comment 15308
-- line 15309: deterministic comment
BEGIN; SELECT 15310; COMMIT;
# hash comment 15311
SELECT * FROM "quoted_15312" WHERE col = E'esc\'15312';
UPDATE bench_t_17 SET payload = 15313 WHERE id = 17;
BEGIN; SELECT 15314; COMMIT;
$dz$ dollar body 15315 ; semicolon inside $dz$
/* block header 15316 */
SELECT `mysql_15317` FROM `tbl_17`;
/* block header 15318 */
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
BEGIN; SELECT 15320; COMMIT;
WITH cte_15321 AS (SELECT 15321 AS n) SELECT n FROM cte_15321;
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
-- line 15323: deterministic comment
-- line 15324: deterministic comment
$dz$ dollar body 15325 ; semicolon inside $dz$
SELECT * FROM "quoted_15326" WHERE col = E'esc\'15326';
DELETE FROM bench_t_31 WHERE id = 15;
# hash comment 15328
SELECT nested FROM t WHERE id IN (15329, 15330, 15331);
UPDATE bench_t_34 SET payload = 15330 WHERE id = 2;
INSERT INTO bench_t_99 (id, payload) VALUES (15331, 'v15331');
UPDATE bench_t_36 SET payload = 15332 WHERE id = 4;
SELECT nested FROM t WHERE id IN (15333, 15334, 15335);
WITH cte_15334 AS (SELECT 15334 AS n) SELECT n FROM cte_15334;
BEGIN; SELECT 15335; COMMIT;
BEGIN; SELECT 15336; COMMIT;
SELECT `mysql_15337` FROM `tbl_37`;
SELECT * FROM "quoted_15338" WHERE col = E'esc\'15338';
$dz$ dollar body 15339 ; semicolon inside $dz$
/* block header 15340 */
BEGIN; SELECT 15341; COMMIT;
/* block header 15342 */
-- line 15343: deterministic comment
INSERT INTO bench_t_112 (id, payload) VALUES (15344, 'v15344');
INSERT INTO bench_t_113 (id, payload) VALUES (15345, 'O''Brien');
SELECT [bracket_15346] FROM [dbo].[tbl_26];
SELECT nested FROM t WHERE id IN (15347, 15348, 15349);
SELECT [bracket_15348] FROM [dbo].[tbl_28];
SELECT nested FROM t WHERE id IN (15349, 15350, 15351);
SELECT 15350 AS id, 'row_15350' AS label;
SELECT nested FROM t WHERE id IN (15351, 15352, 15353);
# hash comment 15352
INSERT INTO bench_t_121 (id, payload) VALUES (15353, 'v15353');
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
# hash comment 15356
# hash comment 15357
SELECT * FROM "quoted_15358" WHERE col = E'esc\'15358';
SELECT nested FROM t WHERE id IN (15359, 15360, 15361);
SELECT nested FROM t WHERE id IN (15360, 15361, 15362);
-- line 15361: deterministic comment
$dz$ dollar body 15362 ; semicolon inside $dz$
SELECT nested FROM t WHERE id IN (15363, 15364, 15365);
SELECT [bracket_15364] FROM [dbo].[tbl_4];
SELECT * FROM "quoted_15365" WHERE col = E'esc\'15365';
SELECT `mysql_15366` FROM `tbl_16`;
DELETE FROM bench_t_7 WHERE id = 7;
SELECT nested FROM t WHERE id IN (15368, 15369, 15370);
SELECT [bracket_15369] FROM [dbo].[tbl_9];
SELECT * FROM "quoted_15370" WHERE col = E'esc\'15370';
$dz$ dollar body 15371 ; semicolon inside $dz$
# hash comment 15372
$dz$ dollar body 15373 ; semicolon inside $dz$
# hash comment 15374
SELECT nested FROM t WHERE id IN (15375, 15376, 15377);
# hash comment 15376
SELECT [bracket_15377] FROM [dbo].[tbl_17];
SELECT [bracket_15378] FROM [dbo].[tbl_18];
BEGIN; SELECT 15379; COMMIT;
SELECT nested FROM t WHERE id IN (15380, 15381, 15382);
SELECT `mysql_15381` FROM `tbl_31`;
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT * FROM "quoted_15383" WHERE col = E'esc\'15383';
BEGIN; SELECT 15384; COMMIT;
WITH cte_15385 AS (SELECT 15385 AS n) SELECT n FROM cte_15385;
SELECT 15386 AS id, 'row_15386' AS label;
SELECT `mysql_15387` FROM `tbl_37`;
# hash comment 15388
SELECT nested FROM t WHERE id IN (15389, 15390, 15391);
WITH cte_15390 AS (SELECT 15390 AS n) SELECT n FROM cte_15390;
DELETE FROM bench_t_31 WHERE id = 15;
# hash comment 15392
BEGIN; SELECT 15393; COMMIT;
-- line 15394: deterministic comment
SELECT * FROM "quoted_15395" WHERE col = E'esc\'15395';
-- line 15396: deterministic comment
UPDATE bench_t_37 SET payload = 15397 WHERE id = 5;
SELECT `mysql_15398` FROM `tbl_48`;
/* block header 15399 */
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
/* block header 15401 */
SELECT `mysql_15402` FROM `tbl_2`;
WITH cte_15403 AS (SELECT 15403 AS n) SELECT n FROM cte_15403;
INSERT INTO bench_t_44 (id, payload) VALUES (15404, 'v15404');
-- line 15405: deterministic comment
UPDATE bench_t_46 SET payload = 15406 WHERE id = 14;
SELECT `mysql_15407` FROM `tbl_7`;
INSERT INTO bench_t_48 (id, payload) VALUES (15408, 'v15408');
/* block header 15409 */
$dz$ dollar body 15410 ; semicolon inside $dz$
SELECT 15411 AS id, 'row_15411' AS label;
# hash comment 15412
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT * FROM "quoted_15414" WHERE col = E'esc\'15414';
# hash comment 15415
SELECT 15416 AS id, 'row_15416' AS label;
SELECT `mysql_15417` FROM `tbl_17`;
INSERT INTO bench_t_58 (id, payload) VALUES (15418, 'v15418');
SELECT `mysql_15419` FROM `tbl_19`;
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT nested FROM t WHERE id IN (15421, 15422, 15423);
/* block header 15422 */
# hash comment 15423
WITH cte_15424 AS (SELECT 15424 AS n) SELECT n FROM cte_15424;
BEGIN; SELECT 15425; COMMIT;
UPDATE bench_t_2 SET payload = 15426 WHERE id = 2;
UPDATE bench_t_3 SET payload = 15427 WHERE id = 3;
SELECT nested FROM t WHERE id IN (15428, 15429, 15430);
WITH cte_15429 AS (SELECT 15429 AS n) SELECT n FROM cte_15429;
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
-- line 15431: deterministic comment
SELECT `mysql_15432` FROM `tbl_32`;
DELETE FROM bench_t_9 WHERE id = 9;
SELECT `mysql_15434` FROM `tbl_34`;
SELECT * FROM "quoted_15435" WHERE col = E'esc\'15435';
# hash comment 15436
/* block header 15437 */
DELETE FROM bench_t_14 WHERE id = 14;
INSERT INTO bench_t_79 (id, payload) VALUES (15439, 'v15439');
-- line 15440: deterministic comment
SELECT 15441 AS id, 'row_15441' AS label;
DELETE FROM bench_t_18 WHERE id = 2;
SELECT 15443 AS id, 'row_15443' AS label;
SELECT nested FROM t WHERE id IN (15444, 15445, 15446);
WITH cte_15445 AS (SELECT 15445 AS n) SELECT n FROM cte_15445;
SELECT `mysql_15446` FROM `tbl_46`;
SELECT [bracket_15447] FROM [dbo].[tbl_7];
SELECT 15448 AS id, 'row_15448' AS label;
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
BEGIN; SELECT 15450; COMMIT;
SELECT 15451 AS id, 'row_15451' AS label;
WITH cte_15452 AS (SELECT 15452 AS n) SELECT n FROM cte_15452;
SELECT 15453 AS id, 'row_15453' AS label;
SELECT 15454 AS id, 'row_15454' AS label;
# hash comment 15455
/* block header 15456 */
SELECT * FROM "quoted_15457" WHERE col = E'esc\'15457';
SELECT 15458 AS id, 'row_15458' AS label;
SELECT * FROM "quoted_15459" WHERE col = E'esc\'15459';
$dz$ dollar body 15460 ; semicolon inside $dz$
# hash comment 15461
SELECT `mysql_15462` FROM `tbl_12`;
WITH cte_15463 AS (SELECT 15463 AS n) SELECT n FROM cte_15463;
SELECT * FROM "quoted_15464" WHERE col = E'esc\'15464';
BEGIN; SELECT 15465; COMMIT;
$dz$ dollar body 15466 ; semicolon inside $dz$
$dz$ dollar body 15467 ; semicolon inside $dz$
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT nested FROM t WHERE id IN (15469, 15470, 15471);
$dz$ dollar body 15470 ; semicolon inside $dz$
/* block header 15471 */
WITH cte_15472 AS (SELECT 15472 AS n) SELECT n FROM cte_15472;
$dz$ dollar body 15473 ; semicolon inside $dz$
-- line 15474: deterministic comment
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
$dz$ dollar body 15476 ; semicolon inside $dz$
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
UPDATE bench_t_54 SET payload = 15478 WHERE id = 22;
-- line 15479: deterministic comment
SELECT 15480 AS id, 'row_15480' AS label;
SELECT * FROM "quoted_15481" WHERE col = E'esc\'15481';
SELECT `mysql_15482` FROM `tbl_32`;
/* block header 15483 */
INSERT INTO bench_t_124 (id, payload) VALUES (15484, 'v15484');
SELECT * FROM "quoted_15485" WHERE col = E'esc\'15485';
WITH cte_15486 AS (SELECT 15486 AS n) SELECT n FROM cte_15486;
UPDATE bench_t_63 SET payload = 15487 WHERE id = 31;
# hash comment 15488
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT 15490 AS id, 'row_15490' AS label;
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
DELETE FROM bench_t_4 WHERE id = 4;
$dz$ dollar body 15493 ; semicolon inside $dz$
SELECT nested FROM t WHERE id IN (15494, 15495, 15496);
# hash comment 15495
BEGIN; SELECT 15496; COMMIT;
INSERT INTO bench_t_9 (id, payload) VALUES (15497, 'v15497');
SELECT `mysql_15498` FROM `tbl_48`;
SELECT * FROM "quoted_15499" WHERE col = E'esc\'15499';
/*
 * section 62
 * checksum 534f
 */
WITH cte_15500 AS (SELECT 15500 AS n) SELECT n FROM cte_15500;
-- line 15505: deterministic comment
# hash comment 15506
INSERT INTO bench_t_19 (id, payload) VALUES (15507, 'v15507');
-- line 15508: deterministic comment
BEGIN; SELECT 15509; COMMIT;
$dz$ dollar body 15510 ; semicolon inside $dz$
SELECT [bracket_15511] FROM [dbo].[tbl_31];
$dz$ dollar body 15512 ; semicolon inside $dz$
SELECT * FROM "quoted_15513" WHERE col = E'esc\'15513';
WITH cte_15514 AS (SELECT 15514 AS n) SELECT n FROM cte_15514;
SELECT * FROM "quoted_15515" WHERE col = E'esc\'15515';
# hash comment 15516
INSERT INTO bench_t_29 (id, payload) VALUES (15517, 'v15517');
SELECT `mysql_15518` FROM `tbl_18`;
DELETE FROM bench_t_31 WHERE id = 15;
SELECT nested FROM t WHERE id IN (15520, 15521, 15522);
SELECT * FROM "quoted_15521" WHERE col = E'esc\'15521';
SELECT nested FROM t WHERE id IN (15522, 15523, 15524);
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT [bracket_15525] FROM [dbo].[tbl_5];
# hash comment 15526
DELETE FROM bench_t_7 WHERE id = 7;
UPDATE bench_t_40 SET payload = 15528 WHERE id = 8;
INSERT INTO bench_t_41 (id, payload) VALUES (15529, 'v15529');
/* block header 15530 */
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
UPDATE bench_t_44 SET payload = 15532 WHERE id = 12;
BEGIN; SELECT 15533; COMMIT;
/* block header 15534 */
SELECT [bracket_15535] FROM [dbo].[tbl_15];
$dz$ dollar body 15536 ; semicolon inside $dz$
/* block header 15537 */
INSERT INTO bench_t_50 (id, payload) VALUES (15538, 'v15538');
DELETE FROM bench_t_19 WHERE id = 3;
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT 15541 AS id, 'row_15541' AS label;
# hash comment 15542
-- line 15543: deterministic comment
SELECT * FROM "quoted_15544" WHERE col = E'esc\'15544';
DELETE FROM bench_t_25 WHERE id = 9;
SELECT 15546 AS id, 'row_15546' AS label;
SELECT [bracket_15547] FROM [dbo].[tbl_27];
SELECT 15548 AS id, 'row_15548' AS label;
-- line 15549: deterministic comment
SELECT [bracket_15550] FROM [dbo].[tbl_30];
$dz$ dollar body 15551 ; semicolon inside $dz$
/* block header 15552 */
SELECT * FROM "quoted_15553" WHERE col = E'esc\'15553';
SELECT `mysql_15554` FROM `tbl_4`;
SELECT nested FROM t WHERE id IN (15555, 15556, 15557);
INSERT INTO bench_t_68 (id, payload) VALUES (15556, 'v15556');
$dz$ dollar body 15557 ; semicolon inside $dz$
/* block header 15558 */
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
DELETE FROM bench_t_8 WHERE id = 8;
# hash comment 15561
SELECT * FROM "quoted_15562" WHERE col = E'esc\'15562';
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
WITH cte_15564 AS (SELECT 15564 AS n) SELECT n FROM cte_15564;
INSERT INTO bench_t_77 (id, payload) VALUES (15565, 'O''Brien');
SELECT `mysql_15566` FROM `tbl_16`;
# hash comment 15567
# hash comment 15568
DELETE FROM bench_t_17 WHERE id = 1;
BEGIN; SELECT 15570; COMMIT;
WITH cte_15571 AS (SELECT 15571 AS n) SELECT n FROM cte_15571;
SELECT [bracket_15572] FROM [dbo].[tbl_12];
UPDATE bench_t_21 SET payload = 15573 WHERE id = 21;
SELECT `mysql_15574` FROM `tbl_24`;
SELECT `mysql_15575` FROM `tbl_25`;
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
# hash comment 15577
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
INSERT INTO bench_t_91 (id, payload) VALUES (15579, 'v15579');
BEGIN; SELECT 15580; COMMIT;
BEGIN; SELECT 15581; COMMIT;
WITH cte_15582 AS (SELECT 15582 AS n) SELECT n FROM cte_15582;
BEGIN; SELECT 15583; COMMIT;
SELECT 15584 AS id, 'row_15584' AS label;
SELECT `mysql_15585` FROM `tbl_35`;
# hash comment 15586
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
-- line 15588: deterministic comment
SELECT * FROM "quoted_15589" WHERE col = E'esc\'15589';
# hash comment 15590
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT nested FROM t WHERE id IN (15592, 15593, 15594);
INSERT INTO bench_t_105 (id, payload) VALUES (15593, 'v15593');
SELECT `mysql_15594` FROM `tbl_44`;
SELECT nested FROM t WHERE id IN (15595, 15596, 15597);
# hash comment 15596
# hash comment 15597
DELETE FROM bench_t_14 WHERE id = 14;
SELECT * FROM "quoted_15599" WHERE col = E'esc\'15599';
# hash comment 15600
UPDATE bench_t_49 SET payload = 15601 WHERE id = 17;
SELECT 15602 AS id, 'row_15602' AS label;
UPDATE bench_t_51 SET payload = 15603 WHERE id = 19;
/* block header 15604 */
INSERT INTO bench_t_117 (id, payload) VALUES (15605, 'v15605');
DELETE FROM bench_t_22 WHERE id = 6;
DELETE FROM bench_t_23 WHERE id = 7;
SELECT `mysql_15608` FROM `tbl_8`;
$dz$ dollar body 15609 ; semicolon inside $dz$
SELECT 15610 AS id, 'row_15610' AS label;
DELETE FROM bench_t_27 WHERE id = 11;
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT `mysql_15613` FROM `tbl_13`;
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
# hash comment 15615
/* block header 15616 */
BEGIN; SELECT 15617; COMMIT;
SELECT nested FROM t WHERE id IN (15618, 15619, 15620);
$dz$ dollar body 15619 ; semicolon inside $dz$
DELETE FROM bench_t_4 WHERE id = 4;
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
WITH cte_15622 AS (SELECT 15622 AS n) SELECT n FROM cte_15622;
DELETE FROM bench_t_7 WHERE id = 7;
/* block header 15624 */
SELECT nested FROM t WHERE id IN (15625, 15626, 15627);
-- line 15626: deterministic comment
SELECT [bracket_15627] FROM [dbo].[tbl_27];
SELECT [bracket_15628] FROM [dbo].[tbl_28];
SELECT [bracket_15629] FROM [dbo].[tbl_29];
SELECT nested FROM t WHERE id IN (15630, 15631, 15632);
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
# hash comment 15632
DELETE FROM bench_t_17 WHERE id = 1;
INSERT INTO bench_t_18 (id, payload) VALUES (15634, 'v15634');
-- line 15635: deterministic comment
SELECT * FROM "quoted_15636" WHERE col = E'esc\'15636';
SELECT * FROM "quoted_15637" WHERE col = E'esc\'15637';
SELECT nested FROM t WHERE id IN (15638, 15639, 15640);
# hash comment 15639
BEGIN; SELECT 15640; COMMIT;
SELECT * FROM "quoted_15641" WHERE col = E'esc\'15641';
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
-- line 15643: deterministic comment
SELECT [bracket_15644] FROM [dbo].[tbl_4];
SELECT * FROM "quoted_15645" WHERE col = E'esc\'15645';
# hash comment 15646
SELECT nested FROM t WHERE id IN (15647, 15648, 15649);
SELECT nested FROM t WHERE id IN (15648, 15649, 15650);
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
UPDATE bench_t_34 SET payload = 15650 WHERE id = 2;
SELECT [bracket_15651] FROM [dbo].[tbl_11];
DELETE FROM bench_t_4 WHERE id = 4;
INSERT INTO bench_t_37 (id, payload) VALUES (15653, 'O''Brien');
SELECT `mysql_15654` FROM `tbl_4`;
SELECT [bracket_15655] FROM [dbo].[tbl_15];
-- line 15656: deterministic comment
SELECT * FROM "quoted_15657" WHERE col = E'esc\'15657';
# hash comment 15658
/* block header 15659 */
SELECT `mysql_15660` FROM `tbl_10`;
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT [bracket_15662] FROM [dbo].[tbl_22];
$dz$ dollar body 15663 ; semicolon inside $dz$
SELECT 15664 AS id, 'row_15664' AS label;
SELECT `mysql_15665` FROM `tbl_15`;
SELECT `mysql_15666` FROM `tbl_16`;
$dz$ dollar body 15667 ; semicolon inside $dz$
SELECT `mysql_15668` FROM `tbl_18`;
INSERT INTO bench_t_53 (id, payload) VALUES (15669, 'v15669');
SELECT [bracket_15670] FROM [dbo].[tbl_30];
/* block header 15671 */
BEGIN; SELECT 15672; COMMIT;
INSERT INTO bench_t_57 (id, payload) VALUES (15673, 'v15673');
/* block header 15674 */
SELECT [bracket_15675] FROM [dbo].[tbl_35];
BEGIN; SELECT 15676; COMMIT;
# hash comment 15677
/* block header 15678 */
WITH cte_15679 AS (SELECT 15679 AS n) SELECT n FROM cte_15679;
$dz$ dollar body 15680 ; semicolon inside $dz$
INSERT INTO bench_t_65 (id, payload) VALUES (15681, 'v15681');
INSERT INTO bench_t_66 (id, payload) VALUES (15682, 'v15682');
/* block header 15683 */
SELECT * FROM "quoted_15684" WHERE col = E'esc\'15684';
SELECT `mysql_15685` FROM `tbl_35`;
WITH cte_15686 AS (SELECT 15686 AS n) SELECT n FROM cte_15686;
SELECT * FROM "quoted_15687" WHERE col = E'esc\'15687';
SELECT [bracket_15688] FROM [dbo].[tbl_8];
SELECT [bracket_15689] FROM [dbo].[tbl_9];
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT * FROM "quoted_15691" WHERE col = E'esc\'15691';
# hash comment 15692
# hash comment 15693
-- line 15694: deterministic comment
UPDATE bench_t_15 SET payload = 15695 WHERE id = 15;
$dz$ dollar body 15696 ; semicolon inside $dz$
INSERT INTO bench_t_81 (id, payload) VALUES (15697, 'O''Brien');
SELECT * FROM "quoted_15698" WHERE col = E'esc\'15698';
INSERT INTO bench_t_83 (id, payload) VALUES (15699, 'v15699');
SELECT * FROM "quoted_15700" WHERE col = E'esc\'15700';
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
UPDATE bench_t_22 SET payload = 15702 WHERE id = 22;
$dz$ dollar body 15703 ; semicolon inside $dz$
SELECT * FROM "quoted_15704" WHERE col = E'esc\'15704';
BEGIN; SELECT 15705; COMMIT;
BEGIN; SELECT 15706; COMMIT;
SELECT [bracket_15707] FROM [dbo].[tbl_27];
INSERT INTO bench_t_92 (id, payload) VALUES (15708, 'O''Brien');
/* block header 15709 */
SELECT * FROM "quoted_15710" WHERE col = E'esc\'15710';
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
WITH cte_15712 AS (SELECT 15712 AS n) SELECT n FROM cte_15712;
SELECT `mysql_15713` FROM `tbl_13`;
INSERT INTO bench_t_98 (id, payload) VALUES (15714, 'v15714');
SELECT nested FROM t WHERE id IN (15715, 15716, 15717);
SELECT 15716 AS id, 'row_15716' AS label;
WITH cte_15717 AS (SELECT 15717 AS n) SELECT n FROM cte_15717;
SELECT * FROM "quoted_15718" WHERE col = E'esc\'15718';
# hash comment 15719
DELETE FROM bench_t_8 WHERE id = 8;
-- line 15721: deterministic comment
$dz$ dollar body 15722 ; semicolon inside $dz$
/* block header 15723 */
SELECT `mysql_15724` FROM `tbl_24`;
$dz$ dollar body 15725 ; semicolon inside $dz$
INSERT INTO bench_t_110 (id, payload) VALUES (15726, 'v15726');
SELECT `mysql_15727` FROM `tbl_27`;
SELECT * FROM "quoted_15728" WHERE col = E'esc\'15728';
SELECT [bracket_15729] FROM [dbo].[tbl_9];
INSERT INTO bench_t_114 (id, payload) VALUES (15730, 'O''Brien');
BEGIN; SELECT 15731; COMMIT;
WITH cte_15732 AS (SELECT 15732 AS n) SELECT n FROM cte_15732;
SELECT 15733 AS id, 'row_15733' AS label;
-- line 15734: deterministic comment
UPDATE bench_t_55 SET payload = 15735 WHERE id = 23;
BEGIN; SELECT 15736; COMMIT;
WITH cte_15737 AS (SELECT 15737 AS n) SELECT n FROM cte_15737;
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
WITH cte_15739 AS (SELECT 15739 AS n) SELECT n FROM cte_15739;
UPDATE bench_t_60 SET payload = 15740 WHERE id = 28;
SELECT 15741 AS id, 'row_15741' AS label;
SELECT * FROM "quoted_15742" WHERE col = E'esc\'15742';
SELECT `mysql_15743` FROM `tbl_43`;
/* block header 15744 */
INSERT INTO bench_t_1 (id, payload) VALUES (15745, 'v15745');
SELECT `mysql_15746` FROM `tbl_46`;
# hash comment 15747
BEGIN; SELECT 15748; COMMIT;
/* block header 15749 */
/*
 * section 63
 * checksum 91f
 */
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
DELETE FROM bench_t_11 WHERE id = 11;
$dz$ dollar body 15756 ; semicolon inside $dz$
/* block header 15757 */
SELECT `mysql_15758` FROM `tbl_8`;
SELECT nested FROM t WHERE id IN (15759, 15760, 15761);
SELECT * FROM "quoted_15760" WHERE col = E'esc\'15760';
SELECT * FROM "quoted_15761" WHERE col = E'esc\'15761';
SELECT * FROM "quoted_15762" WHERE col = E'esc\'15762';
UPDATE bench_t_19 SET payload = 15763 WHERE id = 19;
UPDATE bench_t_20 SET payload = 15764 WHERE id = 20;
SELECT [bracket_15765] FROM [dbo].[tbl_5];
DELETE FROM bench_t_22 WHERE id = 6;
$dz$ dollar body 15767 ; semicolon inside $dz$
BEGIN; SELECT 15768; COMMIT;
DELETE FROM bench_t_25 WHERE id = 9;
DELETE FROM bench_t_26 WHERE id = 10;
SELECT * FROM "quoted_15771" WHERE col = E'esc\'15771';
SELECT `mysql_15772` FROM `tbl_22`;
SELECT 15773 AS id, 'row_15773' AS label;
SELECT nested FROM t WHERE id IN (15774, 15775, 15776);
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT nested FROM t WHERE id IN (15776, 15777, 15778);
INSERT INTO bench_t_33 (id, payload) VALUES (15777, 'v15777');
SELECT * FROM "quoted_15778" WHERE col = E'esc\'15778';
SELECT 15779 AS id, 'row_15779' AS label;
$dz$ dollar body 15780 ; semicolon inside $dz$
$dz$ dollar body 15781 ; semicolon inside $dz$
WITH cte_15782 AS (SELECT 15782 AS n) SELECT n FROM cte_15782;
SELECT nested FROM t WHERE id IN (15783, 15784, 15785);
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT * FROM "quoted_15785" WHERE col = E'esc\'15785';
# hash comment 15786
# hash comment 15787
UPDATE bench_t_44 SET payload = 15788 WHERE id = 12;
DELETE FROM bench_t_13 WHERE id = 13;
WITH cte_15790 AS (SELECT 15790 AS n) SELECT n FROM cte_15790;
INSERT INTO bench_t_47 (id, payload) VALUES (15791, 'v15791');
$dz$ dollar body 15792 ; semicolon inside $dz$
/* block header 15793 */
WITH cte_15794 AS (SELECT 15794 AS n) SELECT n FROM cte_15794;
BEGIN; SELECT 15795; COMMIT;
$dz$ dollar body 15796 ; semicolon inside $dz$
SELECT [bracket_15797] FROM [dbo].[tbl_37];
BEGIN; SELECT 15798; COMMIT;
BEGIN; SELECT 15799; COMMIT;
SELECT 15800 AS id, 'row_15800' AS label;
$dz$ dollar body 15801 ; semicolon inside $dz$
# hash comment 15802
SELECT 15803 AS id, 'row_15803' AS label;
WITH cte_15804 AS (SELECT 15804 AS n) SELECT n FROM cte_15804;
$dz$ dollar body 15805 ; semicolon inside $dz$
UPDATE bench_t_62 SET payload = 15806 WHERE id = 30;
SELECT nested FROM t WHERE id IN (15807, 15808, 15809);
# hash comment 15808
INSERT INTO bench_t_65 (id, payload) VALUES (15809, 'v15809');
WITH cte_15810 AS (SELECT 15810 AS n) SELECT n FROM cte_15810;
DELETE FROM bench_t_3 WHERE id = 3;
SELECT * FROM "quoted_15812" WHERE col = E'esc\'15812';
SELECT nested FROM t WHERE id IN (15813, 15814, 15815);
SELECT 15814 AS id, 'row_15814' AS label;
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
DELETE FROM bench_t_8 WHERE id = 8;
# hash comment 15817
-- line 15818: deterministic comment
SELECT [bracket_15819] FROM [dbo].[tbl_19];
SELECT 15820 AS id, 'row_15820' AS label;
SELECT 15821 AS id, 'row_15821' AS label;
UPDATE bench_t_14 SET payload = 15822 WHERE id = 14;
UPDATE bench_t_15 SET payload = 15823 WHERE id = 15;
SELECT 15824 AS id, 'row_15824' AS label;
SELECT 15825 AS id, 'row_15825' AS label;
SELECT `mysql_15826` FROM `tbl_26`;
SELECT 15827 AS id, 'row_15827' AS label;
DELETE FROM bench_t_20 WHERE id = 4;
BEGIN; SELECT 15829; COMMIT;
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
DELETE FROM bench_t_23 WHERE id = 7;
SELECT nested FROM t WHERE id IN (15832, 15833, 15834);
-- line 15833: deterministic comment
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
$dz$ dollar body 15835 ; semicolon inside $dz$
SELECT * FROM "quoted_15836" WHERE col = E'esc\'15836';
SELECT nested FROM t WHERE id IN (15837, 15838, 15839);
UPDATE bench_t_30 SET payload = 15838 WHERE id = 30;
SELECT * FROM "quoted_15839" WHERE col = E'esc\'15839';
WITH cte_15840 AS (SELECT 15840 AS n) SELECT n FROM cte_15840;
SELECT 15841 AS id, 'row_15841' AS label;
SELECT nested FROM t WHERE id IN (15842, 15843, 15844);
BEGIN; SELECT 15843; COMMIT;
-- line 15844: deterministic comment
SELECT nested FROM t WHERE id IN (15845, 15846, 15847);
/* block header 15846 */
$dz$ dollar body 15847 ; semicolon inside $dz$
SELECT [bracket_15848] FROM [dbo].[tbl_8];
DELETE FROM bench_t_9 WHERE id = 9;
-- line 15850: deterministic comment
SELECT [bracket_15851] FROM [dbo].[tbl_11];
-- line 15852: deterministic comment
SELECT 15853 AS id, 'row_15853' AS label;
SELECT nested FROM t WHERE id IN (15854, 15855, 15856);
SELECT * FROM "quoted_15855" WHERE col = E'esc\'15855';
BEGIN; SELECT 15856; COMMIT;
DELETE FROM bench_t_17 WHERE id = 1;
INSERT INTO bench_t_114 (id, payload) VALUES (15858, 'v15858');
INSERT INTO bench_t_115 (id, payload) VALUES (15859, 'v15859');
SELECT nested FROM t WHERE id IN (15860, 15861, 15862);
SELECT nested FROM t WHERE id IN (15861, 15862, 15863);
WITH cte_15862 AS (SELECT 15862 AS n) SELECT n FROM cte_15862;
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
WITH cte_15864 AS (SELECT 15864 AS n) SELECT n FROM cte_15864;
DELETE FROM bench_t_25 WHERE id = 9;
SELECT 15866 AS id, 'row_15866' AS label;
-- line 15867: deterministic comment
/* block header 15868 */
$dz$ dollar body 15869 ; semicolon inside $dz$
SELECT * FROM "quoted_15870" WHERE col = E'esc\'15870';
SELECT nested FROM t WHERE id IN (15871, 15872, 15873);
SELECT [bracket_15872] FROM [dbo].[tbl_32];
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
-- line 15874: deterministic comment
SELECT * FROM "quoted_15875" WHERE col = E'esc\'15875';
SELECT * FROM "quoted_15876" WHERE col = E'esc\'15876';
SELECT nested FROM t WHERE id IN (15877, 15878, 15879);
# hash comment 15878
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
# hash comment 15880
BEGIN; SELECT 15881; COMMIT;
SELECT `mysql_15882` FROM `tbl_32`;
SELECT * FROM "quoted_15883" WHERE col = E'esc\'15883';
UPDATE bench_t_12 SET payload = 15884 WHERE id = 12;
SELECT * FROM "quoted_15885" WHERE col = E'esc\'15885';
SELECT `mysql_15886` FROM `tbl_36`;
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
INSERT INTO bench_t_17 (id, payload) VALUES (15889, 'v15889');
-- line 15890: deterministic comment
WITH cte_15891 AS (SELECT 15891 AS n) SELECT n FROM cte_15891;
UPDATE bench_t_20 SET payload = 15892 WHERE id = 20;
SELECT 15893 AS id, 'row_15893' AS label;
/* block header 15894 */
INSERT INTO bench_t_23 (id, payload) VALUES (15895, 'O''Brien');
WITH cte_15896 AS (SELECT 15896 AS n) SELECT n FROM cte_15896;
SELECT * FROM "quoted_15897" WHERE col = E'esc\'15897';
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
UPDATE bench_t_28 SET payload = 15900 WHERE id = 28;
SELECT `mysql_15901` FROM `tbl_1`;
UPDATE bench_t_30 SET payload = 15902 WHERE id = 30;
SELECT `mysql_15903` FROM `tbl_3`;
SELECT nested FROM t WHERE id IN (15904, 15905, 15906);
SELECT * FROM "quoted_15905" WHERE col = E'esc\'15905';
$dz$ dollar body 15906 ; semicolon inside $dz$
SELECT * FROM "quoted_15907" WHERE col = E'esc\'15907';
-- line 15908: deterministic comment
$dz$ dollar body 15909 ; semicolon inside $dz$
WITH cte_15910 AS (SELECT 15910 AS n) SELECT n FROM cte_15910;
SELECT [bracket_15911] FROM [dbo].[tbl_31];
SELECT [bracket_15912] FROM [dbo].[tbl_32];
SELECT [bracket_15913] FROM [dbo].[tbl_33];
/* block header 15914 */
-- line 15915: deterministic comment
-- line 15916: deterministic comment
SELECT nested FROM t WHERE id IN (15917, 15918, 15919);
SELECT 15918 AS id, 'row_15918' AS label;
BEGIN; SELECT 15919; COMMIT;
DELETE FROM bench_t_16 WHERE id = 0;
/* block header 15921 */
INSERT INTO bench_t_50 (id, payload) VALUES (15922, 'v15922');
# hash comment 15923
-- line 15924: deterministic comment
SELECT nested FROM t WHERE id IN (15925, 15926, 15927);
DELETE FROM bench_t_22 WHERE id = 6;
# hash comment 15927
WITH cte_15928 AS (SELECT 15928 AS n) SELECT n FROM cte_15928;
$dz$ dollar body 15929 ; semicolon inside $dz$
WITH cte_15930 AS (SELECT 15930 AS n) SELECT n FROM cte_15930;
WITH cte_15931 AS (SELECT 15931 AS n) SELECT n FROM cte_15931;
DELETE FROM bench_t_28 WHERE id = 12;
SELECT 15933 AS id, 'row_15933' AS label;
-- line 15934: deterministic comment
SELECT nested FROM t WHERE id IN (15935, 15936, 15937);
UPDATE bench_t_0 SET payload = 15936 WHERE id = 0;
SELECT * FROM "quoted_15937" WHERE col = E'esc\'15937';
SELECT [bracket_15938] FROM [dbo].[tbl_18];
SELECT * FROM "quoted_15939" WHERE col = E'esc\'15939';
-- line 15940: deterministic comment
DELETE FROM bench_t_5 WHERE id = 5;
SELECT 15942 AS id, 'row_15942' AS label;
DELETE FROM bench_t_7 WHERE id = 7;
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
$dz$ dollar body 15945 ; semicolon inside $dz$
$dz$ dollar body 15946 ; semicolon inside $dz$
SELECT [bracket_15947] FROM [dbo].[tbl_27];
$dz$ dollar body 15948 ; semicolon inside $dz$
SELECT `mysql_15949` FROM `tbl_49`;
SELECT 15950 AS id, 'row_15950' AS label;
SELECT * FROM "quoted_15951" WHERE col = E'esc\'15951';
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT 15953 AS id, 'row_15953' AS label;
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
# hash comment 15955
INSERT INTO bench_t_84 (id, payload) VALUES (15956, 'v15956');
SELECT nested FROM t WHERE id IN (15957, 15958, 15959);
INSERT INTO bench_t_86 (id, payload) VALUES (15958, 'v15958');
-- line 15959: deterministic comment
WITH cte_15960 AS (SELECT 15960 AS n) SELECT n FROM cte_15960;
SELECT 15961 AS id, 'row_15961' AS label;
$dz$ dollar body 15962 ; semicolon inside $dz$
-- line 15963: deterministic comment
BEGIN; SELECT 15964; COMMIT;
SELECT 15965 AS id, 'row_15965' AS label;
WITH cte_15966 AS (SELECT 15966 AS n) SELECT n FROM cte_15966;
SELECT [bracket_15967] FROM [dbo].[tbl_7];
BEGIN; SELECT 15968; COMMIT;
SELECT `mysql_15969` FROM `tbl_19`;
INSERT INTO bench_t_98 (id, payload) VALUES (15970, 'v15970');
SELECT nested FROM t WHERE id IN (15971, 15972, 15973);
-- line 15972: deterministic comment
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
-- line 15974: deterministic comment
BEGIN; SELECT 15975; COMMIT;
SELECT * FROM "quoted_15976" WHERE col = E'esc\'15976';
SELECT `mysql_15977` FROM `tbl_27`;
DELETE FROM bench_t_10 WHERE id = 10;
# hash comment 15979
SELECT `mysql_15980` FROM `tbl_30`;
/* block header 15981 */
SELECT nested FROM t WHERE id IN (15982, 15983, 15984);
SELECT 15983 AS id, 'row_15983' AS label;
SELECT [bracket_15984] FROM [dbo].[tbl_24];
SELECT 15985 AS id, 'row_15985' AS label;
SELECT 15986 AS id, 'row_15986' AS label;
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
WITH cte_15988 AS (SELECT 15988 AS n) SELECT n FROM cte_15988;
SELECT `mysql_15989` FROM `tbl_39`;
SELECT `mysql_15990` FROM `tbl_40`;
/* block header 15991 */
SELECT 15992 AS id, 'row_15992' AS label;
SELECT nested FROM t WHERE id IN (15993, 15994, 15995);
DELETE FROM bench_t_26 WHERE id = 10;
SELECT * FROM "quoted_15995" WHERE col = E'esc\'15995';
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
DELETE FROM bench_t_29 WHERE id = 13;
WITH cte_15998 AS (SELECT 15998 AS n) SELECT n FROM cte_15998;
SELECT [bracket_15999] FROM [dbo].[tbl_39];
/*
 * section 64
 * checksum fba4
 */
/* block header 16000 */
SELECT * FROM "quoted_16005" WHERE col = E'esc\'16005';
-- line 16006: deterministic comment
DELETE FROM bench_t_7 WHERE id = 7;
/* block header 16008 */
INSERT INTO bench_t_9 (id, payload) VALUES (16009, 'v16009');
-- line 16010: deterministic comment
DELETE FROM bench_t_11 WHERE id = 11;
DELETE FROM bench_t_12 WHERE id = 12;
SELECT nested FROM t WHERE id IN (16013, 16014, 16015);
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
BEGIN; SELECT 16015; COMMIT;
# hash comment 16016
INSERT INTO bench_t_17 (id, payload) VALUES (16017, 'v16017');
SELECT [bracket_16018] FROM [dbo].[tbl_18];
SELECT `mysql_16019` FROM `tbl_19`;
SELECT `mysql_16020` FROM `tbl_20`;
SELECT nested FROM t WHERE id IN (16021, 16022, 16023);
$dz$ dollar body 16022 ; semicolon inside $dz$
WITH cte_16023 AS (SELECT 16023 AS n) SELECT n FROM cte_16023;
DELETE FROM bench_t_24 WHERE id = 8;
UPDATE bench_t_25 SET payload = 16025 WHERE id = 25;
SELECT nested FROM t WHERE id IN (16026, 16027, 16028);
SELECT [bracket_16027] FROM [dbo].[tbl_27];
WITH cte_16028 AS (SELECT 16028 AS n) SELECT n FROM cte_16028;
WITH cte_16029 AS (SELECT 16029 AS n) SELECT n FROM cte_16029;
# hash comment 16030
SELECT `mysql_16031` FROM `tbl_31`;
SELECT * FROM "quoted_16032" WHERE col = E'esc\'16032';
SELECT `mysql_16033` FROM `tbl_33`;
SELECT [bracket_16034] FROM [dbo].[tbl_34];
WITH cte_16035 AS (SELECT 16035 AS n) SELECT n FROM cte_16035;
/* block header 16036 */
SELECT `mysql_16037` FROM `tbl_37`;
-- line 16038: deterministic comment
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
UPDATE bench_t_40 SET payload = 16040 WHERE id = 8;
-- line 16041: deterministic comment
SELECT [bracket_16042] FROM [dbo].[tbl_2];
/* block header 16043 */
BEGIN; SELECT 16044; COMMIT;
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT nested FROM t WHERE id IN (16046, 16047, 16048);
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT [bracket_16048] FROM [dbo].[tbl_8];
SELECT 16049 AS id, 'row_16049' AS label;
/* block header 16050 */
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
BEGIN; SELECT 16052; COMMIT;
SELECT * FROM "quoted_16053" WHERE col = E'esc\'16053';
INSERT INTO bench_t_54 (id, payload) VALUES (16054, 'v16054');
SELECT 16055 AS id, 'row_16055' AS label;
# hash comment 16056
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT nested FROM t WHERE id IN (16058, 16059, 16060);
/* block header 16059 */
-- line 16060: deterministic comment
INSERT INTO bench_t_61 (id, payload) VALUES (16061, 'v16061');
$dz$ dollar body 16062 ; semicolon inside $dz$
BEGIN; SELECT 16063; COMMIT;
/* block header 16064 */
BEGIN; SELECT 16065; COMMIT;
BEGIN; SELECT 16066; COMMIT;
SELECT [bracket_16067] FROM [dbo].[tbl_27];
SELECT `mysql_16068` FROM `tbl_18`;
SELECT 16069 AS id, 'row_16069' AS label;
BEGIN; SELECT 16070; COMMIT;
# hash comment 16071
SELECT `mysql_16072` FROM `tbl_22`;
WITH cte_16073 AS (SELECT 16073 AS n) SELECT n FROM cte_16073;
SELECT [bracket_16074] FROM [dbo].[tbl_34];
INSERT INTO bench_t_75 (id, payload) VALUES (16075, 'v16075');
-- line 16076: deterministic comment
UPDATE bench_t_13 SET payload = 16077 WHERE id = 13;
SELECT 16078 AS id, 'row_16078' AS label;
UPDATE bench_t_15 SET payload = 16079 WHERE id = 15;
INSERT INTO bench_t_80 (id, payload) VALUES (16080, 'v16080');
SELECT `mysql_16081` FROM `tbl_31`;
WITH cte_16082 AS (SELECT 16082 AS n) SELECT n FROM cte_16082;
WITH cte_16083 AS (SELECT 16083 AS n) SELECT n FROM cte_16083;
UPDATE bench_t_20 SET payload = 16084 WHERE id = 20;
SELECT [bracket_16085] FROM [dbo].[tbl_5];
DELETE FROM bench_t_22 WHERE id = 6;
UPDATE bench_t_23 SET payload = 16087 WHERE id = 23;
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
$dz$ dollar body 16089 ; semicolon inside $dz$
/* block header 16090 */
/* block header 16091 */
/* block header 16092 */
WITH cte_16093 AS (SELECT 16093 AS n) SELECT n FROM cte_16093;
$dz$ dollar body 16094 ; semicolon inside $dz$
/* block header 16095 */
UPDATE bench_t_32 SET payload = 16096 WHERE id = 0;
-- line 16097: deterministic comment
$dz$ dollar body 16098 ; semicolon inside $dz$
/* block header 16099 */
DELETE FROM bench_t_4 WHERE id = 4;
SELECT [bracket_16101] FROM [dbo].[tbl_21];
WITH cte_16102 AS (SELECT 16102 AS n) SELECT n FROM cte_16102;
DELETE FROM bench_t_7 WHERE id = 7;
$dz$ dollar body 16104 ; semicolon inside $dz$
BEGIN; SELECT 16105; COMMIT;
INSERT INTO bench_t_106 (id, payload) VALUES (16106, 'v16106');
SELECT nested FROM t WHERE id IN (16107, 16108, 16109);
SELECT nested FROM t WHERE id IN (16108, 16109, 16110);
SELECT nested FROM t WHERE id IN (16109, 16110, 16111);
SELECT * FROM "quoted_16110" WHERE col = E'esc\'16110';
SELECT `mysql_16111` FROM `tbl_11`;
SELECT nested FROM t WHERE id IN (16112, 16113, 16114);
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
# hash comment 16114
INSERT INTO bench_t_115 (id, payload) VALUES (16115, 'O''Brien');
WITH cte_16116 AS (SELECT 16116 AS n) SELECT n FROM cte_16116;
UPDATE bench_t_53 SET payload = 16117 WHERE id = 21;
$dz$ dollar body 16118 ; semicolon inside $dz$
DELETE FROM bench_t_23 WHERE id = 7;
DELETE FROM bench_t_24 WHERE id = 8;
UPDATE bench_t_57 SET payload = 16121 WHERE id = 25;
SELECT [bracket_16122] FROM [dbo].[tbl_2];
DELETE FROM bench_t_27 WHERE id = 11;
SELECT 16124 AS id, 'row_16124' AS label;
$dz$ dollar body 16125 ; semicolon inside $dz$
-- line 16126: deterministic comment
-- line 16127: deterministic comment
SELECT [bracket_16128] FROM [dbo].[tbl_8];
# hash comment 16129
INSERT INTO bench_t_2 (id, payload) VALUES (16130, 'v16130');
INSERT INTO bench_t_3 (id, payload) VALUES (16131, 'v16131');
SELECT nested FROM t WHERE id IN (16132, 16133, 16134);
SELECT * FROM "quoted_16133" WHERE col = E'esc\'16133';
/* block header 16134 */
BEGIN; SELECT 16135; COMMIT;
-- line 16136: deterministic comment
DELETE FROM bench_t_9 WHERE id = 9;
SELECT nested FROM t WHERE id IN (16138, 16139, 16140);
INSERT INTO bench_t_11 (id, payload) VALUES (16139, 'v16139');
# hash comment 16140
SELECT nested FROM t WHERE id IN (16141, 16142, 16143);
WITH cte_16142 AS (SELECT 16142 AS n) SELECT n FROM cte_16142;
BEGIN; SELECT 16143; COMMIT;
$dz$ dollar body 16144 ; semicolon inside $dz$
SELECT `mysql_16145` FROM `tbl_45`;
WITH cte_16146 AS (SELECT 16146 AS n) SELECT n FROM cte_16146;
DELETE FROM bench_t_19 WHERE id = 3;
SELECT [bracket_16148] FROM [dbo].[tbl_28];
DELETE FROM bench_t_21 WHERE id = 5;
$dz$ dollar body 16150 ; semicolon inside $dz$
BEGIN; SELECT 16151; COMMIT;
$dz$ dollar body 16152 ; semicolon inside $dz$
SELECT nested FROM t WHERE id IN (16153, 16154, 16155);
DELETE FROM bench_t_26 WHERE id = 10;
$dz$ dollar body 16155 ; semicolon inside $dz$
$dz$ dollar body 16156 ; semicolon inside $dz$
$dz$ dollar body 16157 ; semicolon inside $dz$
DELETE FROM bench_t_30 WHERE id = 14;
WITH cte_16159 AS (SELECT 16159 AS n) SELECT n FROM cte_16159;
WITH cte_16160 AS (SELECT 16160 AS n) SELECT n FROM cte_16160;
WITH cte_16161 AS (SELECT 16161 AS n) SELECT n FROM cte_16161;
-- line 16162: deterministic comment
WITH cte_16163 AS (SELECT 16163 AS n) SELECT n FROM cte_16163;
UPDATE bench_t_36 SET payload = 16164 WHERE id = 4;
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
BEGIN; SELECT 16166; COMMIT;
SELECT nested FROM t WHERE id IN (16167, 16168, 16169);
SELECT * FROM "quoted_16168" WHERE col = E'esc\'16168';
/* block header 16169 */
SELECT 16170 AS id, 'row_16170' AS label;
/* block header 16171 */
$dz$ dollar body 16172 ; semicolon inside $dz$
SELECT [bracket_16173] FROM [dbo].[tbl_13];
# hash comment 16174
SELECT [bracket_16175] FROM [dbo].[tbl_15];
SELECT * FROM "quoted_16176" WHERE col = E'esc\'16176';
DELETE FROM bench_t_17 WHERE id = 1;
/* block header 16178 */
SELECT [bracket_16179] FROM [dbo].[tbl_19];
$dz$ dollar body 16180 ; semicolon inside $dz$
# hash comment 16181
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
INSERT INTO bench_t_55 (id, payload) VALUES (16183, 'v16183');
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
INSERT INTO bench_t_57 (id, payload) VALUES (16185, 'v16185');
# hash comment 16186
SELECT `mysql_16187` FROM `tbl_37`;
DELETE FROM bench_t_28 WHERE id = 12;
/* block header 16189 */
$dz$ dollar body 16190 ; semicolon inside $dz$
/* block header 16191 */
$dz$ dollar body 16192 ; semicolon inside $dz$
UPDATE bench_t_1 SET payload = 16193 WHERE id = 1;
BEGIN; SELECT 16194; COMMIT;
BEGIN; SELECT 16195; COMMIT;
INSERT INTO bench_t_68 (id, payload) VALUES (16196, 'v16196');
INSERT INTO bench_t_69 (id, payload) VALUES (16197, 'v16197');
SELECT nested FROM t WHERE id IN (16198, 16199, 16200);
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
UPDATE bench_t_8 SET payload = 16200 WHERE id = 8;
UPDATE bench_t_9 SET payload = 16201 WHERE id = 9;
SELECT * FROM "quoted_16202" WHERE col = E'esc\'16202';
SELECT * FROM "quoted_16203" WHERE col = E'esc\'16203';
SELECT 16204 AS id, 'row_16204' AS label;
/* block header 16205 */
UPDATE bench_t_14 SET payload = 16206 WHERE id = 14;
DELETE FROM bench_t_15 WHERE id = 15;
DELETE FROM bench_t_16 WHERE id = 0;
UPDATE bench_t_17 SET payload = 16209 WHERE id = 17;
$dz$ dollar body 16210 ; semicolon inside $dz$
$dz$ dollar body 16211 ; semicolon inside $dz$
# hash comment 16212
/* block header 16213 */
UPDATE bench_t_22 SET payload = 16214 WHERE id = 22;
SELECT nested FROM t WHERE id IN (16215, 16216, 16217);
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT nested FROM t WHERE id IN (16217, 16218, 16219);
SELECT * FROM "quoted_16218" WHERE col = E'esc\'16218';
DELETE FROM bench_t_27 WHERE id = 11;
$dz$ dollar body 16220 ; semicolon inside $dz$
WITH cte_16221 AS (SELECT 16221 AS n) SELECT n FROM cte_16221;
/* block header 16222 */
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
-- line 16224: deterministic comment
SELECT * FROM "quoted_16225" WHERE col = E'esc\'16225';
SELECT `mysql_16226` FROM `tbl_26`;
WITH cte_16227 AS (SELECT 16227 AS n) SELECT n FROM cte_16227;
SELECT `mysql_16228` FROM `tbl_28`;
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
DELETE FROM bench_t_6 WHERE id = 6;
SELECT nested FROM t WHERE id IN (16231, 16232, 16233);
SELECT `mysql_16232` FROM `tbl_32`;
SELECT * FROM "quoted_16233" WHERE col = E'esc\'16233';
$dz$ dollar body 16234 ; semicolon inside $dz$
SELECT [bracket_16235] FROM [dbo].[tbl_35];
$dz$ dollar body 16236 ; semicolon inside $dz$
# hash comment 16237
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
INSERT INTO bench_t_111 (id, payload) VALUES (16239, 'v16239');
SELECT * FROM "quoted_16240" WHERE col = E'esc\'16240';
DELETE FROM bench_t_17 WHERE id = 1;
INSERT INTO bench_t_114 (id, payload) VALUES (16242, 'v16242');
WITH cte_16243 AS (SELECT 16243 AS n) SELECT n FROM cte_16243;
DELETE FROM bench_t_20 WHERE id = 4;
DELETE FROM bench_t_21 WHERE id = 5;
SELECT * FROM "quoted_16246" WHERE col = E'esc\'16246';
DELETE FROM bench_t_23 WHERE id = 7;
BEGIN; SELECT 16248; COMMIT;
SELECT 16249 AS id, 'row_16249' AS label;
/*
 * section 65
 * checksum 4387
 */
WITH cte_16250 AS (SELECT 16250 AS n) SELECT n FROM cte_16250;
SELECT `mysql_16255` FROM `tbl_5`;
$dz$ dollar body 16256 ; semicolon inside $dz$
$dz$ dollar body 16257 ; semicolon inside $dz$
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
# hash comment 16259
BEGIN; SELECT 16260; COMMIT;
# hash comment 16261
BEGIN; SELECT 16262; COMMIT;
SELECT 16263 AS id, 'row_16263' AS label;
# hash comment 16264
WITH cte_16265 AS (SELECT 16265 AS n) SELECT n FROM cte_16265;
INSERT INTO bench_t_10 (id, payload) VALUES (16266, 'v16266');
DELETE FROM bench_t_11 WHERE id = 11;
# hash comment 16268
SELECT [bracket_16269] FROM [dbo].[tbl_29];
-- line 16270: deterministic comment
SELECT `mysql_16271` FROM `tbl_21`;
WITH cte_16272 AS (SELECT 16272 AS n) SELECT n FROM cte_16272;
SELECT [bracket_16273] FROM [dbo].[tbl_33];
UPDATE bench_t_18 SET payload = 16274 WHERE id = 18;
SELECT nested FROM t WHERE id IN (16275, 16276, 16277);
BEGIN; SELECT 16276; COMMIT;
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT 16278 AS id, 'row_16278' AS label;
WITH cte_16279 AS (SELECT 16279 AS n) SELECT n FROM cte_16279;
SELECT nested FROM t WHERE id IN (16280, 16281, 16282);
DELETE FROM bench_t_25 WHERE id = 9;
SELECT `mysql_16282` FROM `tbl_32`;
UPDATE bench_t_27 SET payload = 16283 WHERE id = 27;
# hash comment 16284
DELETE FROM bench_t_29 WHERE id = 13;
BEGIN; SELECT 16286; COMMIT;
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
UPDATE bench_t_32 SET payload = 16288 WHERE id = 0;
UPDATE bench_t_33 SET payload = 16289 WHERE id = 1;
SELECT [bracket_16290] FROM [dbo].[tbl_10];
# hash comment 16291
SELECT 16292 AS id, 'row_16292' AS label;
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT 16294 AS id, 'row_16294' AS label;
SELECT [bracket_16295] FROM [dbo].[tbl_15];
SELECT * FROM "quoted_16296" WHERE col = E'esc\'16296';
DELETE FROM bench_t_9 WHERE id = 9;
/* block header 16298 */
BEGIN; SELECT 16299; COMMIT;
SELECT [bracket_16300] FROM [dbo].[tbl_20];
DELETE FROM bench_t_13 WHERE id = 13;
SELECT 16302 AS id, 'row_16302' AS label;
$dz$ dollar body 16303 ; semicolon inside $dz$
# hash comment 16304
/* block header 16305 */
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
WITH cte_16307 AS (SELECT 16307 AS n) SELECT n FROM cte_16307;
$dz$ dollar body 16308 ; semicolon inside $dz$
SELECT `mysql_16309` FROM `tbl_9`;
$dz$ dollar body 16310 ; semicolon inside $dz$
$dz$ dollar body 16311 ; semicolon inside $dz$
$dz$ dollar body 16312 ; semicolon inside $dz$
BEGIN; SELECT 16313; COMMIT;
# hash comment 16314
/* block header 16315 */
$dz$ dollar body 16316 ; semicolon inside $dz$
UPDATE bench_t_61 SET payload = 16317 WHERE id = 29;
WITH cte_16318 AS (SELECT 16318 AS n) SELECT n FROM cte_16318;
/* block header 16319 */
-- line 16320: deterministic comment
SELECT [bracket_16321] FROM [dbo].[tbl_1];
$dz$ dollar body 16322 ; semicolon inside $dz$
BEGIN; SELECT 16323; COMMIT;
UPDATE bench_t_4 SET payload = 16324 WHERE id = 4;
# hash comment 16325
WITH cte_16326 AS (SELECT 16326 AS n) SELECT n FROM cte_16326;
DELETE FROM bench_t_7 WHERE id = 7;
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
DELETE FROM bench_t_9 WHERE id = 9;
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT [bracket_16331] FROM [dbo].[tbl_11];
SELECT * FROM "quoted_16332" WHERE col = E'esc\'16332';
INSERT INTO bench_t_77 (id, payload) VALUES (16333, 'v16333');
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
# hash comment 16335
DELETE FROM bench_t_16 WHERE id = 0;
SELECT [bracket_16337] FROM [dbo].[tbl_17];
# hash comment 16338
BEGIN; SELECT 16339; COMMIT;
SELECT 16340 AS id, 'row_16340' AS label;
$dz$ dollar body 16341 ; semicolon inside $dz$
SELECT * FROM "quoted_16342" WHERE col = E'esc\'16342';
SELECT 16343 AS id, 'row_16343' AS label;
SELECT nested FROM t WHERE id IN (16344, 16345, 16346);
/* block header 16345 */
SELECT * FROM "quoted_16346" WHERE col = E'esc\'16346';
SELECT * FROM "quoted_16347" WHERE col = E'esc\'16347';
-- line 16348: deterministic comment
SELECT `mysql_16349` FROM `tbl_49`;
DELETE FROM bench_t_30 WHERE id = 14;
-- line 16351: deterministic comment
/* block header 16352 */
SELECT * FROM "quoted_16353" WHERE col = E'esc\'16353';
SELECT * FROM "quoted_16354" WHERE col = E'esc\'16354';
BEGIN; SELECT 16355; COMMIT;
/* block header 16356 */
SELECT 16357 AS id, 'row_16357' AS label;
# hash comment 16358
SELECT `mysql_16359` FROM `tbl_9`;
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT nested FROM t WHERE id IN (16361, 16362, 16363);
WITH cte_16362 AS (SELECT 16362 AS n) SELECT n FROM cte_16362;
/* block header 16363 */
SELECT nested FROM t WHERE id IN (16364, 16365, 16366);
DELETE FROM bench_t_13 WHERE id = 13;
SELECT nested FROM t WHERE id IN (16366, 16367, 16368);
$dz$ dollar body 16367 ; semicolon inside $dz$
INSERT INTO bench_t_112 (id, payload) VALUES (16368, 'O''Brien');
SELECT 16369 AS id, 'row_16369' AS label;
# hash comment 16370
BEGIN; SELECT 16371; COMMIT;
DELETE FROM bench_t_20 WHERE id = 4;
WITH cte_16373 AS (SELECT 16373 AS n) SELECT n FROM cte_16373;
# hash comment 16374
DELETE FROM bench_t_23 WHERE id = 7;
SELECT `mysql_16376` FROM `tbl_26`;
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
-- line 16378: deterministic comment
WITH cte_16379 AS (SELECT 16379 AS n) SELECT n FROM cte_16379;
SELECT nested FROM t WHERE id IN (16380, 16381, 16382);
/* block header 16381 */
SELECT `mysql_16382` FROM `tbl_32`;
SELECT [bracket_16383] FROM [dbo].[tbl_23];
# hash comment 16384
SELECT 16385 AS id, 'row_16385' AS label;
$dz$ dollar body 16386 ; semicolon inside $dz$
SELECT nested FROM t WHERE id IN (16387, 16388, 16389);
DELETE FROM bench_t_4 WHERE id = 4;
UPDATE bench_t_5 SET payload = 16389 WHERE id = 5;
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
/* block header 16391 */
$dz$ dollar body 16392 ; semicolon inside $dz$
SELECT nested FROM t WHERE id IN (16393, 16394, 16395);
-- line 16394: deterministic comment
INSERT INTO bench_t_11 (id, payload) VALUES (16395, 'v16395');
-- line 16396: deterministic comment
UPDATE bench_t_13 SET payload = 16397 WHERE id = 13;
UPDATE bench_t_14 SET payload = 16398 WHERE id = 14;
UPDATE bench_t_15 SET payload = 16399 WHERE id = 15;
UPDATE bench_t_16 SET payload = 16400 WHERE id = 16;
$dz$ dollar body 16401 ; semicolon inside $dz$
INSERT INTO bench_t_18 (id, payload) VALUES (16402, 'v16402');
WITH cte_16403 AS (SELECT 16403 AS n) SELECT n FROM cte_16403;
SELECT * FROM "quoted_16404" WHERE col = E'esc\'16404';
-- line 16405: deterministic comment
SELECT [bracket_16406] FROM [dbo].[tbl_6];
SELECT 16407 AS id, 'row_16407' AS label;
WITH cte_16408 AS (SELECT 16408 AS n) SELECT n FROM cte_16408;
UPDATE bench_t_25 SET payload = 16409 WHERE id = 25;
WITH cte_16410 AS (SELECT 16410 AS n) SELECT n FROM cte_16410;
# hash comment 16411
SELECT 16412 AS id, 'row_16412' AS label;
SELECT `mysql_16413` FROM `tbl_13`;
SELECT [bracket_16414] FROM [dbo].[tbl_14];
$dz$ dollar body 16415 ; semicolon inside $dz$
INSERT INTO bench_t_32 (id, payload) VALUES (16416, 'v16416');
DELETE FROM bench_t_1 WHERE id = 1;
# hash comment 16418
SELECT `mysql_16419` FROM `tbl_19`;
BEGIN; SELECT 16420; COMMIT;
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
/* block header 16422 */
$dz$ dollar body 16423 ; semicolon inside $dz$
/* block header 16424 */
/* block header 16425 */
$dz$ dollar body 16426 ; semicolon inside $dz$
WITH cte_16427 AS (SELECT 16427 AS n) SELECT n FROM cte_16427;
WITH cte_16428 AS (SELECT 16428 AS n) SELECT n FROM cte_16428;
$dz$ dollar body 16429 ; semicolon inside $dz$
SELECT * FROM "quoted_16430" WHERE col = E'esc\'16430';
SELECT nested FROM t WHERE id IN (16431, 16432, 16433);
$dz$ dollar body 16432 ; semicolon inside $dz$
SELECT 16433 AS id, 'row_16433' AS label;
SELECT * FROM "quoted_16434" WHERE col = E'esc\'16434';
SELECT * FROM "quoted_16435" WHERE col = E'esc\'16435';
WITH cte_16436 AS (SELECT 16436 AS n) SELECT n FROM cte_16436;
$dz$ dollar body 16437 ; semicolon inside $dz$
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
-- line 16439: deterministic comment
INSERT INTO bench_t_56 (id, payload) VALUES (16440, 'v16440');
SELECT `mysql_16441` FROM `tbl_41`;
DELETE FROM bench_t_26 WHERE id = 10;
UPDATE bench_t_59 SET payload = 16443 WHERE id = 27;
-- line 16444: deterministic comment
BEGIN; SELECT 16445; COMMIT;
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT nested FROM t WHERE id IN (16447, 16448, 16449);
SELECT `mysql_16448` FROM `tbl_48`;
INSERT INTO bench_t_65 (id, payload) VALUES (16449, 'v16449');
BEGIN; SELECT 16450; COMMIT;
# hash comment 16451
INSERT INTO bench_t_68 (id, payload) VALUES (16452, 'v16452');
$dz$ dollar body 16453 ; semicolon inside $dz$
SELECT 16454 AS id, 'row_16454' AS label;
WITH cte_16455 AS (SELECT 16455 AS n) SELECT n FROM cte_16455;
SELECT [bracket_16456] FROM [dbo].[tbl_16];
SELECT `mysql_16457` FROM `tbl_7`;
# hash comment 16458
SELECT 16459 AS id, 'row_16459' AS label;
SELECT [bracket_16460] FROM [dbo].[tbl_20];
SELECT 16461 AS id, 'row_16461' AS label;
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
INSERT INTO bench_t_79 (id, payload) VALUES (16463, 'v16463');
BEGIN; SELECT 16464; COMMIT;
INSERT INTO bench_t_81 (id, payload) VALUES (16465, 'v16465');
BEGIN; SELECT 16466; COMMIT;
UPDATE bench_t_19 SET payload = 16467 WHERE id = 19;
# hash comment 16468
SELECT 16469 AS id, 'row_16469' AS label;
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
INSERT INTO bench_t_87 (id, payload) VALUES (16471, 'v16471');
/* block header 16472 */
$dz$ dollar body 16473 ; semicolon inside $dz$
SELECT * FROM "quoted_16474" WHERE col = E'esc\'16474';
SELECT 16475 AS id, 'row_16475' AS label;
BEGIN; SELECT 16476; COMMIT;
/* block header 16477 */
SELECT 16478 AS id, 'row_16478' AS label;
SELECT nested FROM t WHERE id IN (16479, 16480, 16481);
WITH cte_16480 AS (SELECT 16480 AS n) SELECT n FROM cte_16480;
INSERT INTO bench_t_97 (id, payload) VALUES (16481, 'v16481');
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT * FROM "quoted_16484" WHERE col = E'esc\'16484';
SELECT * FROM "quoted_16485" WHERE col = E'esc\'16485';
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
UPDATE bench_t_39 SET payload = 16487 WHERE id = 7;
UPDATE bench_t_40 SET payload = 16488 WHERE id = 8;
SELECT `mysql_16489` FROM `tbl_39`;
SELECT `mysql_16490` FROM `tbl_40`;
SELECT `mysql_16491` FROM `tbl_41`;
# hash comment 16492
$dz$ dollar body 16493 ; semicolon inside $dz$
SELECT * FROM "quoted_16494" WHERE col = E'esc\'16494';
SELECT nested FROM t WHERE id IN (16495, 16496, 16497);
/* block header 16496 */
# hash comment 16497
SELECT [bracket_16498] FROM [dbo].[tbl_18];
SELECT `mysql_16499` FROM `tbl_49`;
/*
 * section 66
 * checksum a094
 */
SELECT * FROM "quoted_16500" WHERE col = E'esc\'16500';
DELETE FROM bench_t_25 WHERE id = 9;
-- line 16506: deterministic comment
/* block header 16507 */
SELECT 16508 AS id, 'row_16508' AS label;
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
# hash comment 16510
$dz$ dollar body 16511 ; semicolon inside $dz$
WITH cte_16512 AS (SELECT 16512 AS n) SELECT n FROM cte_16512;
SELECT `mysql_16513` FROM `tbl_13`;
# hash comment 16514
# hash comment 16515
INSERT INTO bench_t_4 (id, payload) VALUES (16516, 'v16516');
/* block header 16517 */
UPDATE bench_t_6 SET payload = 16518 WHERE id = 6;
UPDATE bench_t_7 SET payload = 16519 WHERE id = 7;
SELECT [bracket_16520] FROM [dbo].[tbl_0];
# hash comment 16521
WITH cte_16522 AS (SELECT 16522 AS n) SELECT n FROM cte_16522;
$dz$ dollar body 16523 ; semicolon inside $dz$
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
UPDATE bench_t_13 SET payload = 16525 WHERE id = 13;
UPDATE bench_t_14 SET payload = 16526 WHERE id = 14;
SELECT * FROM "quoted_16527" WHERE col = E'esc\'16527';
/* block header 16528 */
SELECT `mysql_16529` FROM `tbl_29`;
INSERT INTO bench_t_18 (id, payload) VALUES (16530, 'v16530');
SELECT nested FROM t WHERE id IN (16531, 16532, 16533);
/* block header 16532 */
SELECT nested FROM t WHERE id IN (16533, 16534, 16535);
UPDATE bench_t_22 SET payload = 16534 WHERE id = 22;
SELECT `mysql_16535` FROM `tbl_35`;
-- line 16536: deterministic comment
SELECT `mysql_16537` FROM `tbl_37`;
SELECT 16538 AS id, 'row_16538' AS label;
INSERT INTO bench_t_27 (id, payload) VALUES (16539, 'v16539');
SELECT 16540 AS id, 'row_16540' AS label;
-- line 16541: deterministic comment
SELECT `mysql_16542` FROM `tbl_42`;
WITH cte_16543 AS (SELECT 16543 AS n) SELECT n FROM cte_16543;
SELECT [bracket_16544] FROM [dbo].[tbl_24];
SELECT [bracket_16545] FROM [dbo].[tbl_25];
$dz$ dollar body 16546 ; semicolon inside $dz$
-- line 16547: deterministic comment
SELECT * FROM "quoted_16548" WHERE col = E'esc\'16548';
/* block header 16549 */
UPDATE bench_t_38 SET payload = 16550 WHERE id = 6;
SELECT `mysql_16551` FROM `tbl_1`;
BEGIN; SELECT 16552; COMMIT;
UPDATE bench_t_41 SET payload = 16553 WHERE id = 9;
UPDATE bench_t_42 SET payload = 16554 WHERE id = 10;
-- line 16555: deterministic comment
SELECT [bracket_16556] FROM [dbo].[tbl_36];
SELECT 16557 AS id, 'row_16557' AS label;
$dz$ dollar body 16558 ; semicolon inside $dz$
SELECT 16559 AS id, 'row_16559' AS label;
BEGIN; SELECT 16560; COMMIT;
# hash comment 16561
SELECT * FROM "quoted_16562" WHERE col = E'esc\'16562';
-- line 16563: deterministic comment
SELECT 16564 AS id, 'row_16564' AS label;
WITH cte_16565 AS (SELECT 16565 AS n) SELECT n FROM cte_16565;
# hash comment 16566
INSERT INTO bench_t_55 (id, payload) VALUES (16567, 'v16567');
BEGIN; SELECT 16568; COMMIT;
WITH cte_16569 AS (SELECT 16569 AS n) SELECT n FROM cte_16569;
/* block header 16570 */
BEGIN; SELECT 16571; COMMIT;
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
INSERT INTO bench_t_61 (id, payload) VALUES (16573, 'v16573');
SELECT [bracket_16574] FROM [dbo].[tbl_14];
DELETE FROM bench_t_31 WHERE id = 15;
$dz$ dollar body 16576 ; semicolon inside $dz$
SELECT nested FROM t WHERE id IN (16577, 16578, 16579);
SELECT 16578 AS id, 'row_16578' AS label;
-- line 16579: deterministic comment
SELECT `mysql_16580` FROM `tbl_30`;
SELECT nested FROM t WHERE id IN (16581, 16582, 16583);
SELECT nested FROM t WHERE id IN (16582, 16583, 16584);
/* block header 16583 */
/* block header 16584 */
SELECT * FROM "quoted_16585" WHERE col = E'esc\'16585';
BEGIN; SELECT 16586; COMMIT;
SELECT * FROM "quoted_16587" WHERE col = E'esc\'16587';
UPDATE bench_t_12 SET payload = 16588 WHERE id = 12;
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
BEGIN; SELECT 16590; COMMIT;
/* block header 16591 */
UPDATE bench_t_16 SET payload = 16592 WHERE id = 16;
SELECT 16593 AS id, 'row_16593' AS label;
UPDATE bench_t_18 SET payload = 16594 WHERE id = 18;
SELECT 16595 AS id, 'row_16595' AS label;
SELECT [bracket_16596] FROM [dbo].[tbl_36];
BEGIN; SELECT 16597; COMMIT;
SELECT * FROM "quoted_16598" WHERE col = E'esc\'16598';
-- line 16599: deterministic comment
SELECT `mysql_16600` FROM `tbl_0`;
SELECT [bracket_16601] FROM [dbo].[tbl_1];
SELECT [bracket_16602] FROM [dbo].[tbl_2];
UPDATE bench_t_27 SET payload = 16603 WHERE id = 27;
/* block header 16604 */
/* block header 16605 */
/* block header 16606 */
WITH cte_16607 AS (SELECT 16607 AS n) SELECT n FROM cte_16607;
-- line 16608: deterministic comment
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
WITH cte_16610 AS (SELECT 16610 AS n) SELECT n FROM cte_16610;
SELECT nested FROM t WHERE id IN (16611, 16612, 16613);
SELECT `mysql_16612` FROM `tbl_12`;
SELECT * FROM "quoted_16613" WHERE col = E'esc\'16613';
-- line 16614: deterministic comment
SELECT `mysql_16615` FROM `tbl_15`;
SELECT * FROM "quoted_16616" WHERE col = E'esc\'16616';
# hash comment 16617
$dz$ dollar body 16618 ; semicolon inside $dz$
DELETE FROM bench_t_11 WHERE id = 11;
INSERT INTO bench_t_108 (id, payload) VALUES (16620, 'v16620');
DELETE FROM bench_t_13 WHERE id = 13;
SELECT * FROM "quoted_16622" WHERE col = E'esc\'16622';
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT [bracket_16624] FROM [dbo].[tbl_24];
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT * FROM "quoted_16626" WHERE col = E'esc\'16626';
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
/* block header 16628 */
SELECT `mysql_16629` FROM `tbl_29`;
UPDATE bench_t_54 SET payload = 16630 WHERE id = 22;
DELETE FROM bench_t_23 WHERE id = 7;
SELECT * FROM "quoted_16632" WHERE col = E'esc\'16632';
SELECT 16633 AS id, 'row_16633' AS label;
SELECT [bracket_16634] FROM [dbo].[tbl_34];
$dz$ dollar body 16635 ; semicolon inside $dz$
-- line 16636: deterministic comment
-- line 16637: deterministic comment
# hash comment 16638
UPDATE bench_t_63 SET payload = 16639 WHERE id = 31;
SELECT 16640 AS id, 'row_16640' AS label;
SELECT * FROM "quoted_16641" WHERE col = E'esc\'16641';
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
DELETE FROM bench_t_3 WHERE id = 3;
# hash comment 16644
SELECT 16645 AS id, 'row_16645' AS label;
WITH cte_16646 AS (SELECT 16646 AS n) SELECT n FROM cte_16646;
SELECT nested FROM t WHERE id IN (16647, 16648, 16649);
$dz$ dollar body 16648 ; semicolon inside $dz$
SELECT `mysql_16649` FROM `tbl_49`;
SELECT [bracket_16650] FROM [dbo].[tbl_10];
$dz$ dollar body 16651 ; semicolon inside $dz$
UPDATE bench_t_12 SET payload = 16652 WHERE id = 12;
# hash comment 16653
SELECT [bracket_16654] FROM [dbo].[tbl_14];
SELECT [bracket_16655] FROM [dbo].[tbl_15];
SELECT `mysql_16656` FROM `tbl_6`;
INSERT INTO bench_t_17 (id, payload) VALUES (16657, 'v16657');
SELECT * FROM "quoted_16658" WHERE col = E'esc\'16658';
SELECT [bracket_16659] FROM [dbo].[tbl_19];
$dz$ dollar body 16660 ; semicolon inside $dz$
SELECT * FROM "quoted_16661" WHERE col = E'esc\'16661';
WITH cte_16662 AS (SELECT 16662 AS n) SELECT n FROM cte_16662;
-- line 16663: deterministic comment
INSERT INTO bench_t_24 (id, payload) VALUES (16664, 'v16664');
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
# hash comment 16666
# hash comment 16667
UPDATE bench_t_28 SET payload = 16668 WHERE id = 28;
# hash comment 16669
WITH cte_16670 AS (SELECT 16670 AS n) SELECT n FROM cte_16670;
$dz$ dollar body 16671 ; semicolon inside $dz$
/* block header 16672 */
SELECT nested FROM t WHERE id IN (16673, 16674, 16675);
DELETE FROM bench_t_2 WHERE id = 2;
UPDATE bench_t_35 SET payload = 16675 WHERE id = 3;
UPDATE bench_t_36 SET payload = 16676 WHERE id = 4;
/* block header 16677 */
SELECT nested FROM t WHERE id IN (16678, 16679, 16680);
BEGIN; SELECT 16679; COMMIT;
INSERT INTO bench_t_40 (id, payload) VALUES (16680, 'v16680');
WITH cte_16681 AS (SELECT 16681 AS n) SELECT n FROM cte_16681;
DELETE FROM bench_t_10 WHERE id = 10;
WITH cte_16683 AS (SELECT 16683 AS n) SELECT n FROM cte_16683;
SELECT [bracket_16684] FROM [dbo].[tbl_4];
SELECT [bracket_16685] FROM [dbo].[tbl_5];
SELECT [bracket_16686] FROM [dbo].[tbl_6];
/* block header 16687 */
$dz$ dollar body 16688 ; semicolon inside $dz$
WITH cte_16689 AS (SELECT 16689 AS n) SELECT n FROM cte_16689;
UPDATE bench_t_50 SET payload = 16690 WHERE id = 18;
SELECT 16691 AS id, 'row_16691' AS label;
DELETE FROM bench_t_20 WHERE id = 4;
# hash comment 16693
-- line 16694: deterministic comment
-- line 16695: deterministic comment
BEGIN; SELECT 16696; COMMIT;
INSERT INTO bench_t_57 (id, payload) VALUES (16697, 'v16697');
DELETE FROM bench_t_26 WHERE id = 10;
INSERT INTO bench_t_59 (id, payload) VALUES (16699, 'v16699');
SELECT `mysql_16700` FROM `tbl_0`;
# hash comment 16701
DELETE FROM bench_t_30 WHERE id = 14;
$dz$ dollar body 16703 ; semicolon inside $dz$
# hash comment 16704
BEGIN; SELECT 16705; COMMIT;
INSERT INTO bench_t_66 (id, payload) VALUES (16706, 'v16706');
DELETE FROM bench_t_3 WHERE id = 3;
SELECT 16708 AS id, 'row_16708' AS label;
-- line 16709: deterministic comment
# hash comment 16710
SELECT `mysql_16711` FROM `tbl_11`;
SELECT * FROM "quoted_16712" WHERE col = E'esc\'16712';
BEGIN; SELECT 16713; COMMIT;
SELECT nested FROM t WHERE id IN (16714, 16715, 16716);
INSERT INTO bench_t_75 (id, payload) VALUES (16715, 'v16715');
SELECT nested FROM t WHERE id IN (16716, 16717, 16718);
SELECT `mysql_16717` FROM `tbl_17`;
SELECT nested FROM t WHERE id IN (16718, 16719, 16720);
WITH cte_16719 AS (SELECT 16719 AS n) SELECT n FROM cte_16719;
BEGIN; SELECT 16720; COMMIT;
/* block header 16721 */
-- line 16722: deterministic comment
# hash comment 16723
BEGIN; SELECT 16724; COMMIT;
SELECT nested FROM t WHERE id IN (16725, 16726, 16727);
SELECT * FROM "quoted_16726" WHERE col = E'esc\'16726';
SELECT `mysql_16727` FROM `tbl_27`;
UPDATE bench_t_24 SET payload = 16728 WHERE id = 24;
SELECT * FROM "quoted_16729" WHERE col = E'esc\'16729';
UPDATE bench_t_26 SET payload = 16730 WHERE id = 26;
WITH cte_16731 AS (SELECT 16731 AS n) SELECT n FROM cte_16731;
DELETE FROM bench_t_28 WHERE id = 12;
SELECT 16733 AS id, 'row_16733' AS label;
BEGIN; SELECT 16734; COMMIT;
INSERT INTO bench_t_95 (id, payload) VALUES (16735, 'v16735');
# hash comment 16736
UPDATE bench_t_33 SET payload = 16737 WHERE id = 1;
-- line 16738: deterministic comment
DELETE FROM bench_t_3 WHERE id = 3;
SELECT nested FROM t WHERE id IN (16740, 16741, 16742);
/* block header 16741 */
DELETE FROM bench_t_6 WHERE id = 6;
$dz$ dollar body 16743 ; semicolon inside $dz$
-- line 16744: deterministic comment
DELETE FROM bench_t_9 WHERE id = 9;
/* block header 16746 */
$dz$ dollar body 16747 ; semicolon inside $dz$
-- line 16748: deterministic comment
/* block header 16749 */
/*
 * section 67
 * checksum 16ea
 */
# hash comment 16750
SELECT `mysql_16755` FROM `tbl_5`;
DELETE FROM bench_t_20 WHERE id = 4;
DELETE FROM bench_t_21 WHERE id = 5;
SELECT 16758 AS id, 'row_16758' AS label;
SELECT * FROM "quoted_16759" WHERE col = E'esc\'16759';
# hash comment 16760
DELETE FROM bench_t_25 WHERE id = 9;
# hash comment 16762
WITH cte_16763 AS (SELECT 16763 AS n) SELECT n FROM cte_16763;
# hash comment 16764
/* block header 16765 */
UPDATE bench_t_62 SET payload = 16766 WHERE id = 30;
/* block header 16767 */
UPDATE bench_t_0 SET payload = 16768 WHERE id = 0;
SELECT 16769 AS id, 'row_16769' AS label;
SELECT nested FROM t WHERE id IN (16770, 16771, 16772);
SELECT nested FROM t WHERE id IN (16771, 16772, 16773);
SELECT * FROM "quoted_16772" WHERE col = E'esc\'16772';
BEGIN; SELECT 16773; COMMIT;
BEGIN; SELECT 16774; COMMIT;
WITH cte_16775 AS (SELECT 16775 AS n) SELECT n FROM cte_16775;
SELECT 16776 AS id, 'row_16776' AS label;
WITH cte_16777 AS (SELECT 16777 AS n) SELECT n FROM cte_16777;
SELECT * FROM "quoted_16778" WHERE col = E'esc\'16778';
DELETE FROM bench_t_11 WHERE id = 11;
-- line 16780: deterministic comment
SELECT `mysql_16781` FROM `tbl_31`;
SELECT `mysql_16782` FROM `tbl_32`;
$dz$ dollar body 16783 ; semicolon inside $dz$
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT nested FROM t WHERE id IN (16785, 16786, 16787);
SELECT nested FROM t WHERE id IN (16786, 16787, 16788);
DELETE FROM bench_t_19 WHERE id = 3;
SELECT 16788 AS id, 'row_16788' AS label;
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT * FROM "quoted_16790" WHERE col = E'esc\'16790';
UPDATE bench_t_23 SET payload = 16791 WHERE id = 23;
-- line 16792: deterministic comment
/* block header 16793 */
$dz$ dollar body 16794 ; semicolon inside $dz$
-- line 16795: deterministic comment
SELECT 16796 AS id, 'row_16796' AS label;
WITH cte_16797 AS (SELECT 16797 AS n) SELECT n FROM cte_16797;
# hash comment 16798
DELETE FROM bench_t_31 WHERE id = 15;
SELECT `mysql_16800` FROM `tbl_0`;
SELECT nested FROM t WHERE id IN (16801, 16802, 16803);
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT 16803 AS id, 'row_16803' AS label;
$dz$ dollar body 16804 ; semicolon inside $dz$
UPDATE bench_t_37 SET payload = 16805 WHERE id = 5;
$dz$ dollar body 16806 ; semicolon inside $dz$
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT [bracket_16808] FROM [dbo].[tbl_8];
BEGIN; SELECT 16809; COMMIT;
SELECT `mysql_16810` FROM `tbl_10`;
$dz$ dollar body 16811 ; semicolon inside $dz$
$dz$ dollar body 16812 ; semicolon inside $dz$
$dz$ dollar body 16813 ; semicolon inside $dz$
SELECT 16814 AS id, 'row_16814' AS label;
INSERT INTO bench_t_47 (id, payload) VALUES (16815, 'v16815');
SELECT nested FROM t WHERE id IN (16816, 16817, 16818);
DELETE FROM bench_t_17 WHERE id = 1;
SELECT * FROM "quoted_16818" WHERE col = E'esc\'16818';
# hash comment 16819
WITH cte_16820 AS (SELECT 16820 AS n) SELECT n FROM cte_16820;
SELECT * FROM "quoted_16821" WHERE col = E'esc\'16821';
-- line 16822: deterministic comment
UPDATE bench_t_55 SET payload = 16823 WHERE id = 23;
SELECT nested FROM t WHERE id IN (16824, 16825, 16826);
SELECT 16825 AS id, 'row_16825' AS label;
WITH cte_16826 AS (SELECT 16826 AS n) SELECT n FROM cte_16826;
-- line 16827: deterministic comment
/* block header 16828 */
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
DELETE FROM bench_t_30 WHERE id = 14;
INSERT INTO bench_t_63 (id, payload) VALUES (16831, 'v16831');
SELECT [bracket_16832] FROM [dbo].[tbl_32];
/* block header 16833 */
# hash comment 16834
SELECT * FROM "quoted_16835" WHERE col = E'esc\'16835';
SELECT 16836 AS id, 'row_16836' AS label;
# hash comment 16837
SELECT 16838 AS id, 'row_16838' AS label;
BEGIN; SELECT 16839; COMMIT;
SELECT 16840 AS id, 'row_16840' AS label;
/* block header 16841 */
DELETE FROM bench_t_10 WHERE id = 10;
SELECT * FROM "quoted_16843" WHERE col = E'esc\'16843';
SELECT 16844 AS id, 'row_16844' AS label;
UPDATE bench_t_13 SET payload = 16845 WHERE id = 13;
SELECT nested FROM t WHERE id IN (16846, 16847, 16848);
UPDATE bench_t_15 SET payload = 16847 WHERE id = 15;
BEGIN; SELECT 16848; COMMIT;
# hash comment 16849
WITH cte_16850 AS (SELECT 16850 AS n) SELECT n FROM cte_16850;
DELETE FROM bench_t_19 WHERE id = 3;
SELECT [bracket_16852] FROM [dbo].[tbl_12];
SELECT * FROM "quoted_16853" WHERE col = E'esc\'16853';
INSERT INTO bench_t_86 (id, payload) VALUES (16854, 'v16854');
WITH cte_16855 AS (SELECT 16855 AS n) SELECT n FROM cte_16855;
BEGIN; SELECT 16856; COMMIT;
SELECT [bracket_16857] FROM [dbo].[tbl_17];
BEGIN; SELECT 16858; COMMIT;
BEGIN; SELECT 16859; COMMIT;
DELETE FROM bench_t_28 WHERE id = 12;
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
$dz$ dollar body 16862 ; semicolon inside $dz$
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
WITH cte_16864 AS (SELECT 16864 AS n) SELECT n FROM cte_16864;
SELECT nested FROM t WHERE id IN (16865, 16866, 16867);
SELECT [bracket_16866] FROM [dbo].[tbl_26];
INSERT INTO bench_t_99 (id, payload) VALUES (16867, 'v16867');
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
INSERT INTO bench_t_101 (id, payload) VALUES (16869, 'v16869');
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT * FROM "quoted_16871" WHERE col = E'esc\'16871';
SELECT [bracket_16872] FROM [dbo].[tbl_32];
SELECT `mysql_16873` FROM `tbl_23`;
BEGIN; SELECT 16874; COMMIT;
/* block header 16875 */
SELECT [bracket_16876] FROM [dbo].[tbl_36];
WITH cte_16877 AS (SELECT 16877 AS n) SELECT n FROM cte_16877;
SELECT 16878 AS id, 'row_16878' AS label;
SELECT 16879 AS id, 'row_16879' AS label;
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT [bracket_16881] FROM [dbo].[tbl_1];
DELETE FROM bench_t_18 WHERE id = 2;
SELECT `mysql_16883` FROM `tbl_33`;
# hash comment 16884
SELECT [bracket_16885] FROM [dbo].[tbl_5];
DELETE FROM bench_t_22 WHERE id = 6;
-- line 16887: deterministic comment
WITH cte_16888 AS (SELECT 16888 AS n) SELECT n FROM cte_16888;
BEGIN; SELECT 16889; COMMIT;
$dz$ dollar body 16890 ; semicolon inside $dz$
WITH cte_16891 AS (SELECT 16891 AS n) SELECT n FROM cte_16891;
SELECT * FROM "quoted_16892" WHERE col = E'esc\'16892';
INSERT INTO bench_t_125 (id, payload) VALUES (16893, 'v16893');
SELECT `mysql_16894` FROM `tbl_44`;
SELECT 16895 AS id, 'row_16895' AS label;
SELECT nested FROM t WHERE id IN (16896, 16897, 16898);
BEGIN; SELECT 16897; COMMIT;
DELETE FROM bench_t_2 WHERE id = 2;
SELECT nested FROM t WHERE id IN (16899, 16900, 16901);
SELECT `mysql_16900` FROM `tbl_0`;
SELECT nested FROM t WHERE id IN (16901, 16902, 16903);
/* block header 16902 */
WITH cte_16903 AS (SELECT 16903 AS n) SELECT n FROM cte_16903;
SELECT nested FROM t WHERE id IN (16904, 16905, 16906);
-- line 16905: deterministic comment
INSERT INTO bench_t_10 (id, payload) VALUES (16906, 'v16906');
# hash comment 16907
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
UPDATE bench_t_13 SET payload = 16909 WHERE id = 13;
/* block header 16910 */
# hash comment 16911
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
/* block header 16913 */
SELECT nested FROM t WHERE id IN (16914, 16915, 16916);
-- line 16915: deterministic comment
SELECT `mysql_16916` FROM `tbl_16`;
INSERT INTO bench_t_21 (id, payload) VALUES (16917, 'v16917');
SELECT `mysql_16918` FROM `tbl_18`;
SELECT * FROM "quoted_16919" WHERE col = E'esc\'16919';
SELECT nested FROM t WHERE id IN (16920, 16921, 16922);
WITH cte_16921 AS (SELECT 16921 AS n) SELECT n FROM cte_16921;
/* block header 16922 */
SELECT nested FROM t WHERE id IN (16923, 16924, 16925);
INSERT INTO bench_t_28 (id, payload) VALUES (16924, 'v16924');
# hash comment 16925
SELECT [bracket_16926] FROM [dbo].[tbl_6];
-- line 16927: deterministic comment
SELECT [bracket_16928] FROM [dbo].[tbl_8];
SELECT * FROM "quoted_16929" WHERE col = E'esc\'16929';
INSERT INTO bench_t_34 (id, payload) VALUES (16930, 'v16930');
BEGIN; SELECT 16931; COMMIT;
# hash comment 16932
BEGIN; SELECT 16933; COMMIT;
WITH cte_16934 AS (SELECT 16934 AS n) SELECT n FROM cte_16934;
WITH cte_16935 AS (SELECT 16935 AS n) SELECT n FROM cte_16935;
SELECT nested FROM t WHERE id IN (16936, 16937, 16938);
-- line 16937: deterministic comment
SELECT * FROM "quoted_16938" WHERE col = E'esc\'16938';
-- line 16939: deterministic comment
SELECT `mysql_16940` FROM `tbl_40`;
WITH cte_16941 AS (SELECT 16941 AS n) SELECT n FROM cte_16941;
-- line 16942: deterministic comment
WITH cte_16943 AS (SELECT 16943 AS n) SELECT n FROM cte_16943;
-- line 16944: deterministic comment
BEGIN; SELECT 16945; COMMIT;
DELETE FROM bench_t_18 WHERE id = 2;
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
UPDATE bench_t_52 SET payload = 16948 WHERE id = 20;
SELECT nested FROM t WHERE id IN (16949, 16950, 16951);
# hash comment 16950
/* block header 16951 */
/* block header 16952 */
INSERT INTO bench_t_57 (id, payload) VALUES (16953, 'v16953');
-- line 16954: deterministic comment
$dz$ dollar body 16955 ; semicolon inside $dz$
INSERT INTO bench_t_60 (id, payload) VALUES (16956, 'v16956');
WITH cte_16957 AS (SELECT 16957 AS n) SELECT n FROM cte_16957;
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT `mysql_16959` FROM `tbl_9`;
# hash comment 16960
/* block header 16961 */
# hash comment 16962
-- line 16963: deterministic comment
DELETE FROM bench_t_4 WHERE id = 4;
SELECT * FROM "quoted_16965" WHERE col = E'esc\'16965';
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
# hash comment 16967
WITH cte_16968 AS (SELECT 16968 AS n) SELECT n FROM cte_16968;
$dz$ dollar body 16969 ; semicolon inside $dz$
-- line 16970: deterministic comment
-- line 16971: deterministic comment
BEGIN; SELECT 16972; COMMIT;
SELECT nested FROM t WHERE id IN (16973, 16974, 16975);
INSERT INTO bench_t_78 (id, payload) VALUES (16974, 'v16974');
SELECT [bracket_16975] FROM [dbo].[tbl_15];
SELECT 16976 AS id, 'row_16976' AS label;
WITH cte_16977 AS (SELECT 16977 AS n) SELECT n FROM cte_16977;
SELECT [bracket_16978] FROM [dbo].[tbl_18];
$dz$ dollar body 16979 ; semicolon inside $dz$
SELECT nested FROM t WHERE id IN (16980, 16981, 16982);
SELECT [bracket_16981] FROM [dbo].[tbl_21];
SELECT `mysql_16982` FROM `tbl_32`;
-- line 16983: deterministic comment
UPDATE bench_t_24 SET payload = 16984 WHERE id = 24;
SELECT nested FROM t WHERE id IN (16985, 16986, 16987);
/* block header 16986 */
SELECT nested FROM t WHERE id IN (16987, 16988, 16989);
-- line 16988: deterministic comment
INSERT INTO bench_t_93 (id, payload) VALUES (16989, 'v16989');
SELECT * FROM "quoted_16990" WHERE col = E'esc\'16990';
-- line 16991: deterministic comment
DELETE FROM bench_t_0 WHERE id = 0;
DELETE FROM bench_t_1 WHERE id = 1;
$dz$ dollar body 16994 ; semicolon inside $dz$
-- line 16995: deterministic comment
INSERT INTO bench_t_100 (id, payload) VALUES (16996, 'v16996');
BEGIN; SELECT 16997; COMMIT;
$dz$ dollar body 16998 ; semicolon inside $dz$
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
/*
 * section 68
 * checksum 8d9e
 */
DELETE FROM bench_t_8 WHERE id = 8;
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
# hash comment 17006
# hash comment 17007
-- line 17008: deterministic comment
SELECT [bracket_17009] FROM [dbo].[tbl_9];
SELECT * FROM "quoted_17010" WHERE col = E'esc\'17010';
/* block header 17011 */
SELECT `mysql_17012` FROM `tbl_12`;
-- line 17013: deterministic comment
# hash comment 17014
WITH cte_17015 AS (SELECT 17015 AS n) SELECT n FROM cte_17015;
-- line 17016: deterministic comment
SELECT [bracket_17017] FROM [dbo].[tbl_17];
INSERT INTO bench_t_122 (id, payload) VALUES (17018, 'v17018');
UPDATE bench_t_59 SET payload = 17019 WHERE id = 27;
/* block header 17020 */
SELECT `mysql_17021` FROM `tbl_21`;
-- line 17022: deterministic comment
SELECT `mysql_17023` FROM `tbl_23`;
SELECT nested FROM t WHERE id IN (17024, 17025, 17026);
SELECT [bracket_17025] FROM [dbo].[tbl_25];
UPDATE bench_t_2 SET payload = 17026 WHERE id = 2;
INSERT INTO bench_t_3 (id, payload) VALUES (17027, 'v17027');
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
WITH cte_17029 AS (SELECT 17029 AS n) SELECT n FROM cte_17029;
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
$dz$ dollar body 17031 ; semicolon inside $dz$
-- line 17032: deterministic comment
$dz$ dollar body 17033 ; semicolon inside $dz$
$dz$ dollar body 17034 ; semicolon inside $dz$
-- line 17035: deterministic comment
UPDATE bench_t_12 SET payload = 17036 WHERE id = 12;
DELETE FROM bench_t_13 WHERE id = 13;
# hash comment 17038
BEGIN; SELECT 17039; COMMIT;
$dz$ dollar body 17040 ; semicolon inside $dz$
DELETE FROM bench_t_17 WHERE id = 1;
BEGIN; SELECT 17042; COMMIT;
BEGIN; SELECT 17043; COMMIT;
UPDATE bench_t_20 SET payload = 17044 WHERE id = 20;
-- line 17045: deterministic comment
WITH cte_17046 AS (SELECT 17046 AS n) SELECT n FROM cte_17046;
-- line 17047: deterministic comment
BEGIN; SELECT 17048; COMMIT;
BEGIN; SELECT 17049; COMMIT;
DELETE FROM bench_t_26 WHERE id = 10;
SELECT 17051 AS id, 'row_17051' AS label;
SELECT [bracket_17052] FROM [dbo].[tbl_12];
INSERT INTO bench_t_29 (id, payload) VALUES (17053, 'v17053');
INSERT INTO bench_t_30 (id, payload) VALUES (17054, 'v17054');
SELECT 17055 AS id, 'row_17055' AS label;
SELECT [bracket_17056] FROM [dbo].[tbl_16];
DELETE FROM bench_t_1 WHERE id = 1;
# hash comment 17058
DELETE FROM bench_t_3 WHERE id = 3;
WITH cte_17060 AS (SELECT 17060 AS n) SELECT n FROM cte_17060;
SELECT nested FROM t WHERE id IN (17061, 17062, 17063);
SELECT `mysql_17062` FROM `tbl_12`;
SELECT [bracket_17063] FROM [dbo].[tbl_23];
SELECT nested FROM t WHERE id IN (17064, 17065, 17066);
-- line 17065: deterministic comment
SELECT nested FROM t WHERE id IN (17066, 17067, 17068);
-- line 17067: deterministic comment
# hash comment 17068
SELECT nested FROM t WHERE id IN (17069, 17070, 17071);
DELETE FROM bench_t_14 WHERE id = 14;
SELECT `mysql_17071` FROM `tbl_21`;
SELECT * FROM "quoted_17072" WHERE col = E'esc\'17072';
SELECT * FROM "quoted_17073" WHERE col = E'esc\'17073';
BEGIN; SELECT 17074; COMMIT;
# hash comment 17075
UPDATE bench_t_52 SET payload = 17076 WHERE id = 20;
SELECT * FROM "quoted_17077" WHERE col = E'esc\'17077';
SELECT [bracket_17078] FROM [dbo].[tbl_38];
-- line 17079: deterministic comment
WITH cte_17080 AS (SELECT 17080 AS n) SELECT n FROM cte_17080;
WITH cte_17081 AS (SELECT 17081 AS n) SELECT n FROM cte_17081;
UPDATE bench_t_58 SET payload = 17082 WHERE id = 26;
$dz$ dollar body 17083 ; semicolon inside $dz$
SELECT `mysql_17084` FROM `tbl_34`;
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
UPDATE bench_t_62 SET payload = 17086 WHERE id = 30;
DELETE FROM bench_t_31 WHERE id = 15;
SELECT * FROM "quoted_17088" WHERE col = E'esc\'17088';
$dz$ dollar body 17089 ; semicolon inside $dz$
# hash comment 17090
SELECT 17091 AS id, 'row_17091' AS label;
BEGIN; SELECT 17092; COMMIT;
WITH cte_17093 AS (SELECT 17093 AS n) SELECT n FROM cte_17093;
DELETE FROM bench_t_6 WHERE id = 6;
WITH cte_17095 AS (SELECT 17095 AS n) SELECT n FROM cte_17095;
SELECT `mysql_17096` FROM `tbl_46`;
BEGIN; SELECT 17097; COMMIT;
SELECT [bracket_17098] FROM [dbo].[tbl_18];
SELECT [bracket_17099] FROM [dbo].[tbl_19];
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
WITH cte_17101 AS (SELECT 17101 AS n) SELECT n FROM cte_17101;
INSERT INTO bench_t_78 (id, payload) VALUES (17102, 'v17102');
-- line 17103: deterministic comment
SELECT * FROM "quoted_17104" WHERE col = E'esc\'17104';
-- line 17105: deterministic comment
# hash comment 17106
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
UPDATE bench_t_20 SET payload = 17108 WHERE id = 20;
# hash comment 17109
BEGIN; SELECT 17110; COMMIT;
SELECT [bracket_17111] FROM [dbo].[tbl_31];
DELETE FROM bench_t_24 WHERE id = 8;
WITH cte_17113 AS (SELECT 17113 AS n) SELECT n FROM cte_17113;
SELECT * FROM "quoted_17114" WHERE col = E'esc\'17114';
/* block header 17115 */
SELECT `mysql_17116` FROM `tbl_16`;
# hash comment 17117
SELECT [bracket_17118] FROM [dbo].[tbl_38];
INSERT INTO bench_t_95 (id, payload) VALUES (17119, 'v17119');
# hash comment 17120
SELECT [bracket_17121] FROM [dbo].[tbl_1];
WITH cte_17122 AS (SELECT 17122 AS n) SELECT n FROM cte_17122;
SELECT * FROM "quoted_17123" WHERE col = E'esc\'17123';
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
DELETE FROM bench_t_5 WHERE id = 5;
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
WITH cte_17127 AS (SELECT 17127 AS n) SELECT n FROM cte_17127;
WITH cte_17128 AS (SELECT 17128 AS n) SELECT n FROM cte_17128;
SELECT [bracket_17129] FROM [dbo].[tbl_9];
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
/* block header 17131 */
SELECT * FROM "quoted_17132" WHERE col = E'esc\'17132';
INSERT INTO bench_t_109 (id, payload) VALUES (17133, 'v17133');
SELECT nested FROM t WHERE id IN (17134, 17135, 17136);
SELECT * FROM "quoted_17135" WHERE col = E'esc\'17135';
SELECT nested FROM t WHERE id IN (17136, 17137, 17138);
# hash comment 17137
SELECT 17138 AS id, 'row_17138' AS label;
/* block header 17139 */
$dz$ dollar body 17140 ; semicolon inside $dz$
SELECT [bracket_17141] FROM [dbo].[tbl_21];
WITH cte_17142 AS (SELECT 17142 AS n) SELECT n FROM cte_17142;
SELECT * FROM "quoted_17143" WHERE col = E'esc\'17143';
SELECT nested FROM t WHERE id IN (17144, 17145, 17146);
# hash comment 17145
SELECT * FROM "quoted_17146" WHERE col = E'esc\'17146';
# hash comment 17147
SELECT 17148 AS id, 'row_17148' AS label;
SELECT 17149 AS id, 'row_17149' AS label;
SELECT 17150 AS id, 'row_17150' AS label;
-- line 17151: deterministic comment
SELECT * FROM "quoted_17152" WHERE col = E'esc\'17152';
SELECT nested FROM t WHERE id IN (17153, 17154, 17155);
SELECT * FROM "quoted_17154" WHERE col = E'esc\'17154';
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
-- line 17156: deterministic comment
SELECT nested FROM t WHERE id IN (17157, 17158, 17159);
INSERT INTO bench_t_6 (id, payload) VALUES (17158, 'v17158');
BEGIN; SELECT 17159; COMMIT;
SELECT nested FROM t WHERE id IN (17160, 17161, 17162);
BEGIN; SELECT 17161; COMMIT;
UPDATE bench_t_10 SET payload = 17162 WHERE id = 10;
INSERT INTO bench_t_11 (id, payload) VALUES (17163, 'v17163');
-- line 17164: deterministic comment
SELECT 17165 AS id, 'row_17165' AS label;
SELECT * FROM "quoted_17166" WHERE col = E'esc\'17166';
# hash comment 17167
UPDATE bench_t_16 SET payload = 17168 WHERE id = 16;
-- line 17169: deterministic comment
/* block header 17170 */
/* block header 17171 */
SELECT 17172 AS id, 'row_17172' AS label;
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT [bracket_17174] FROM [dbo].[tbl_14];
UPDATE bench_t_23 SET payload = 17175 WHERE id = 23;
SELECT nested FROM t WHERE id IN (17176, 17177, 17178);
SELECT nested FROM t WHERE id IN (17177, 17178, 17179);
SELECT * FROM "quoted_17178" WHERE col = E'esc\'17178';
# hash comment 17179
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
$dz$ dollar body 17181 ; semicolon inside $dz$
SELECT [bracket_17182] FROM [dbo].[tbl_22];
DELETE FROM bench_t_31 WHERE id = 15;
/* block header 17184 */
$dz$ dollar body 17185 ; semicolon inside $dz$
INSERT INTO bench_t_34 (id, payload) VALUES (17186, 'v17186');
DELETE FROM bench_t_3 WHERE id = 3;
SELECT 17188 AS id, 'row_17188' AS label;
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
DELETE FROM bench_t_6 WHERE id = 6;
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT `mysql_17192` FROM `tbl_42`;
INSERT INTO bench_t_41 (id, payload) VALUES (17193, 'O''Brien');
INSERT INTO bench_t_42 (id, payload) VALUES (17194, 'v17194');
SELECT * FROM "quoted_17195" WHERE col = E'esc\'17195';
SELECT nested FROM t WHERE id IN (17196, 17197, 17198);
INSERT INTO bench_t_45 (id, payload) VALUES (17197, 'v17197');
WITH cte_17198 AS (SELECT 17198 AS n) SELECT n FROM cte_17198;
SELECT nested FROM t WHERE id IN (17199, 17200, 17201);
SELECT [bracket_17200] FROM [dbo].[tbl_0];
SELECT 17201 AS id, 'row_17201' AS label;
SELECT 17202 AS id, 'row_17202' AS label;
UPDATE bench_t_51 SET payload = 17203 WHERE id = 19;
/* block header 17204 */
-- line 17205: deterministic comment
SELECT `mysql_17206` FROM `tbl_6`;
SELECT * FROM "quoted_17207" WHERE col = E'esc\'17207';
# hash comment 17208
$dz$ dollar body 17209 ; semicolon inside $dz$
-- line 17210: deterministic comment
$dz$ dollar body 17211 ; semicolon inside $dz$
SELECT 17212 AS id, 'row_17212' AS label;
$dz$ dollar body 17213 ; semicolon inside $dz$
DELETE FROM bench_t_30 WHERE id = 14;
/* block header 17215 */
/* block header 17216 */
SELECT * FROM "quoted_17217" WHERE col = E'esc\'17217';
SELECT nested FROM t WHERE id IN (17218, 17219, 17220);
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT * FROM "quoted_17221" WHERE col = E'esc\'17221';
SELECT 17222 AS id, 'row_17222' AS label;
INSERT INTO bench_t_71 (id, payload) VALUES (17223, 'v17223');
SELECT [bracket_17224] FROM [dbo].[tbl_24];
DELETE FROM bench_t_9 WHERE id = 9;
BEGIN; SELECT 17226; COMMIT;
UPDATE bench_t_11 SET payload = 17227 WHERE id = 11;
UPDATE bench_t_12 SET payload = 17228 WHERE id = 12;
$dz$ dollar body 17229 ; semicolon inside $dz$
BEGIN; SELECT 17230; COMMIT;
SELECT * FROM "quoted_17231" WHERE col = E'esc\'17231';
WITH cte_17232 AS (SELECT 17232 AS n) SELECT n FROM cte_17232;
INSERT INTO bench_t_81 (id, payload) VALUES (17233, 'v17233');
/* block header 17234 */
SELECT nested FROM t WHERE id IN (17235, 17236, 17237);
SELECT [bracket_17236] FROM [dbo].[tbl_36];
SELECT nested FROM t WHERE id IN (17237, 17238, 17239);
$dz$ dollar body 17238 ; semicolon inside $dz$
INSERT INTO bench_t_87 (id, payload) VALUES (17239, 'v17239');
SELECT nested FROM t WHERE id IN (17240, 17241, 17242);
SELECT nested FROM t WHERE id IN (17241, 17242, 17243);
-- line 17242: deterministic comment
$dz$ dollar body 17243 ; semicolon inside $dz$
$dz$ dollar body 17244 ; semicolon inside $dz$
$dz$ dollar body 17245 ; semicolon inside $dz$
DELETE FROM bench_t_30 WHERE id = 14;
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
BEGIN; SELECT 17248; COMMIT;
DELETE FROM bench_t_1 WHERE id = 1;
/*
 * section 69
 * checksum 9e84
 */
UPDATE bench_t_34 SET payload = 17250 WHERE id = 2;
SELECT * FROM "quoted_17255" WHERE col = E'esc\'17255';
# hash comment 17256
INSERT INTO bench_t_105 (id, payload) VALUES (17257, 'v17257');
/* block header 17258 */
SELECT nested FROM t WHERE id IN (17259, 17260, 17261);
INSERT INTO bench_t_108 (id, payload) VALUES (17260, 'v17260');
UPDATE bench_t_45 SET payload = 17261 WHERE id = 13;
$dz$ dollar body 17262 ; semicolon inside $dz$
SELECT `mysql_17263` FROM `tbl_13`;
$dz$ dollar body 17264 ; semicolon inside $dz$
DELETE FROM bench_t_17 WHERE id = 1;
SELECT [bracket_17266] FROM [dbo].[tbl_26];
BEGIN; SELECT 17267; COMMIT;
# hash comment 17268
INSERT INTO bench_t_117 (id, payload) VALUES (17269, 'v17269');
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
WITH cte_17271 AS (SELECT 17271 AS n) SELECT n FROM cte_17271;
SELECT `mysql_17272` FROM `tbl_22`;
INSERT INTO bench_t_121 (id, payload) VALUES (17273, 'v17273');
WITH cte_17274 AS (SELECT 17274 AS n) SELECT n FROM cte_17274;
SELECT 17275 AS id, 'row_17275' AS label;
INSERT INTO bench_t_124 (id, payload) VALUES (17276, 'v17276');
# hash comment 17277
DELETE FROM bench_t_30 WHERE id = 14;
SELECT nested FROM t WHERE id IN (17279, 17280, 17281);
SELECT 17280 AS id, 'row_17280' AS label;
UPDATE bench_t_1 SET payload = 17281 WHERE id = 1;
WITH cte_17282 AS (SELECT 17282 AS n) SELECT n FROM cte_17282;
/* block header 17283 */
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT 17285 AS id, 'row_17285' AS label;
BEGIN; SELECT 17286; COMMIT;
-- line 17287: deterministic comment
# hash comment 17288
SELECT nested FROM t WHERE id IN (17289, 17290, 17291);
SELECT * FROM "quoted_17290" WHERE col = E'esc\'17290';
-- line 17291: deterministic comment
# hash comment 17292
$dz$ dollar body 17293 ; semicolon inside $dz$
INSERT INTO bench_t_14 (id, payload) VALUES (17294, 'v17294');
BEGIN; SELECT 17295; COMMIT;
WITH cte_17296 AS (SELECT 17296 AS n) SELECT n FROM cte_17296;
SELECT [bracket_17297] FROM [dbo].[tbl_17];
# hash comment 17298
SELECT 17299 AS id, 'row_17299' AS label;
BEGIN; SELECT 17300; COMMIT;
DELETE FROM bench_t_21 WHERE id = 5;
INSERT INTO bench_t_22 (id, payload) VALUES (17302, 'v17302');
SELECT `mysql_17303` FROM `tbl_3`;
$dz$ dollar body 17304 ; semicolon inside $dz$
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
WITH cte_17306 AS (SELECT 17306 AS n) SELECT n FROM cte_17306;
-- line 17307: deterministic comment
DELETE FROM bench_t_28 WHERE id = 12;
-- line 17309: deterministic comment
UPDATE bench_t_30 SET payload = 17310 WHERE id = 30;
SELECT `mysql_17311` FROM `tbl_11`;
-- line 17312: deterministic comment
UPDATE bench_t_33 SET payload = 17313 WHERE id = 1;
# hash comment 17314
# hash comment 17315
WITH cte_17316 AS (SELECT 17316 AS n) SELECT n FROM cte_17316;
SELECT nested FROM t WHERE id IN (17317, 17318, 17319);
SELECT nested FROM t WHERE id IN (17318, 17319, 17320);
SELECT nested FROM t WHERE id IN (17319, 17320, 17321);
# hash comment 17320
DELETE FROM bench_t_9 WHERE id = 9;
SELECT [bracket_17322] FROM [dbo].[tbl_2];
SELECT * FROM "quoted_17323" WHERE col = E'esc\'17323';
BEGIN; SELECT 17324; COMMIT;
SELECT * FROM "quoted_17325" WHERE col = E'esc\'17325';
SELECT nested FROM t WHERE id IN (17326, 17327, 17328);
SELECT nested FROM t WHERE id IN (17327, 17328, 17329);
SELECT [bracket_17328] FROM [dbo].[tbl_8];
# hash comment 17329
SELECT 17330 AS id, 'row_17330' AS label;
SELECT nested FROM t WHERE id IN (17331, 17332, 17333);
SELECT nested FROM t WHERE id IN (17332, 17333, 17334);
INSERT INTO bench_t_53 (id, payload) VALUES (17333, 'v17333');
UPDATE bench_t_54 SET payload = 17334 WHERE id = 22;
SELECT * FROM "quoted_17335" WHERE col = E'esc\'17335';
# hash comment 17336
WITH cte_17337 AS (SELECT 17337 AS n) SELECT n FROM cte_17337;
SELECT 17338 AS id, 'row_17338' AS label;
DELETE FROM bench_t_27 WHERE id = 11;
# hash comment 17340
WITH cte_17341 AS (SELECT 17341 AS n) SELECT n FROM cte_17341;
UPDATE bench_t_62 SET payload = 17342 WHERE id = 30;
SELECT 17343 AS id, 'row_17343' AS label;
DELETE FROM bench_t_0 WHERE id = 0;
UPDATE bench_t_1 SET payload = 17345 WHERE id = 1;
SELECT [bracket_17346] FROM [dbo].[tbl_26];
INSERT INTO bench_t_67 (id, payload) VALUES (17347, 'O''Brien');
SELECT [bracket_17348] FROM [dbo].[tbl_28];
INSERT INTO bench_t_69 (id, payload) VALUES (17349, 'v17349');
UPDATE bench_t_6 SET payload = 17350 WHERE id = 6;
-- line 17351: deterministic comment
SELECT nested FROM t WHERE id IN (17352, 17353, 17354);
SELECT nested FROM t WHERE id IN (17353, 17354, 17355);
SELECT nested FROM t WHERE id IN (17354, 17355, 17356);
/* block header 17355 */
SELECT `mysql_17356` FROM `tbl_6`;
# hash comment 17357
$dz$ dollar body 17358 ; semicolon inside $dz$
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
-- line 17360: deterministic comment
/* block header 17361 */
SELECT `mysql_17362` FROM `tbl_12`;
SELECT * FROM "quoted_17363" WHERE col = E'esc\'17363';
$dz$ dollar body 17364 ; semicolon inside $dz$
SELECT [bracket_17365] FROM [dbo].[tbl_5];
SELECT * FROM "quoted_17366" WHERE col = E'esc\'17366';
UPDATE bench_t_23 SET payload = 17367 WHERE id = 23;
DELETE FROM bench_t_24 WHERE id = 8;
WITH cte_17369 AS (SELECT 17369 AS n) SELECT n FROM cte_17369;
DELETE FROM bench_t_26 WHERE id = 10;
/* block header 17371 */
INSERT INTO bench_t_92 (id, payload) VALUES (17372, 'v17372');
DELETE FROM bench_t_29 WHERE id = 13;
SELECT nested FROM t WHERE id IN (17374, 17375, 17376);
UPDATE bench_t_31 SET payload = 17375 WHERE id = 31;
UPDATE bench_t_32 SET payload = 17376 WHERE id = 0;
# hash comment 17377
SELECT * FROM "quoted_17378" WHERE col = E'esc\'17378';
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT 17380 AS id, 'row_17380' AS label;
DELETE FROM bench_t_5 WHERE id = 5;
$dz$ dollar body 17382 ; semicolon inside $dz$
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
-- line 17384: deterministic comment
SELECT 17385 AS id, 'row_17385' AS label;
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT 17387 AS id, 'row_17387' AS label;
/* block header 17388 */
/* block header 17389 */
SELECT `mysql_17390` FROM `tbl_40`;
SELECT nested FROM t WHERE id IN (17391, 17392, 17393);
/* block header 17392 */
/* block header 17393 */
SELECT [bracket_17394] FROM [dbo].[tbl_34];
/* block header 17395 */
SELECT * FROM "quoted_17396" WHERE col = E'esc\'17396';
SELECT nested FROM t WHERE id IN (17397, 17398, 17399);
SELECT nested FROM t WHERE id IN (17398, 17399, 17400);
$dz$ dollar body 17399 ; semicolon inside $dz$
INSERT INTO bench_t_120 (id, payload) VALUES (17400, 'v17400');
WITH cte_17401 AS (SELECT 17401 AS n) SELECT n FROM cte_17401;
SELECT nested FROM t WHERE id IN (17402, 17403, 17404);
$dz$ dollar body 17403 ; semicolon inside $dz$
BEGIN; SELECT 17404; COMMIT;
# hash comment 17405
SELECT 17406 AS id, 'row_17406' AS label;
WITH cte_17407 AS (SELECT 17407 AS n) SELECT n FROM cte_17407;
SELECT * FROM "quoted_17408" WHERE col = E'esc\'17408';
-- line 17409: deterministic comment
INSERT INTO bench_t_2 (id, payload) VALUES (17410, 'v17410');
UPDATE bench_t_3 SET payload = 17411 WHERE id = 3;
$dz$ dollar body 17412 ; semicolon inside $dz$
WITH cte_17413 AS (SELECT 17413 AS n) SELECT n FROM cte_17413;
SELECT 17414 AS id, 'row_17414' AS label;
/* block header 17415 */
$dz$ dollar body 17416 ; semicolon inside $dz$
SELECT [bracket_17417] FROM [dbo].[tbl_17];
/* block header 17418 */
SELECT nested FROM t WHERE id IN (17419, 17420, 17421);
WITH cte_17420 AS (SELECT 17420 AS n) SELECT n FROM cte_17420;
UPDATE bench_t_13 SET payload = 17421 WHERE id = 13;
SELECT `mysql_17422` FROM `tbl_22`;
WITH cte_17423 AS (SELECT 17423 AS n) SELECT n FROM cte_17423;
SELECT * FROM "quoted_17424" WHERE col = E'esc\'17424';
-- line 17425: deterministic comment
SELECT 17426 AS id, 'row_17426' AS label;
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
-- line 17428: deterministic comment
WITH cte_17429 AS (SELECT 17429 AS n) SELECT n FROM cte_17429;
/* block header 17430 */
SELECT * FROM "quoted_17431" WHERE col = E'esc\'17431';
-- line 17432: deterministic comment
SELECT [bracket_17433] FROM [dbo].[tbl_33];
BEGIN; SELECT 17434; COMMIT;
SELECT * FROM "quoted_17435" WHERE col = E'esc\'17435';
SELECT [bracket_17436] FROM [dbo].[tbl_36];
DELETE FROM bench_t_29 WHERE id = 13;
WITH cte_17438 AS (SELECT 17438 AS n) SELECT n FROM cte_17438;
BEGIN; SELECT 17439; COMMIT;
INSERT INTO bench_t_32 (id, payload) VALUES (17440, 'v17440');
# hash comment 17441
DELETE FROM bench_t_2 WHERE id = 2;
SELECT nested FROM t WHERE id IN (17443, 17444, 17445);
BEGIN; SELECT 17444; COMMIT;
DELETE FROM bench_t_5 WHERE id = 5;
SELECT * FROM "quoted_17446" WHERE col = E'esc\'17446';
$dz$ dollar body 17447 ; semicolon inside $dz$
-- line 17448: deterministic comment
-- line 17449: deterministic comment
BEGIN; SELECT 17450; COMMIT;
-- line 17451: deterministic comment
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
DELETE FROM bench_t_13 WHERE id = 13;
-- line 17454: deterministic comment
SELECT [bracket_17455] FROM [dbo].[tbl_15];
/* block header 17456 */
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
UPDATE bench_t_50 SET payload = 17458 WHERE id = 18;
SELECT 17459 AS id, 'row_17459' AS label;
WITH cte_17460 AS (SELECT 17460 AS n) SELECT n FROM cte_17460;
DELETE FROM bench_t_21 WHERE id = 5;
WITH cte_17462 AS (SELECT 17462 AS n) SELECT n FROM cte_17462;
-- line 17463: deterministic comment
SELECT 17464 AS id, 'row_17464' AS label;
SELECT * FROM "quoted_17465" WHERE col = E'esc\'17465';
SELECT `mysql_17466` FROM `tbl_16`;
/* block header 17467 */
DELETE FROM bench_t_28 WHERE id = 12;
SELECT [bracket_17469] FROM [dbo].[tbl_29];
WITH cte_17470 AS (SELECT 17470 AS n) SELECT n FROM cte_17470;
WITH cte_17471 AS (SELECT 17471 AS n) SELECT n FROM cte_17471;
$dz$ dollar body 17472 ; semicolon inside $dz$
SELECT [bracket_17473] FROM [dbo].[tbl_33];
UPDATE bench_t_2 SET payload = 17474 WHERE id = 2;
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
INSERT INTO bench_t_68 (id, payload) VALUES (17476, 'v17476');
DELETE FROM bench_t_5 WHERE id = 5;
BEGIN; SELECT 17478; COMMIT;
SELECT 17479 AS id, 'row_17479' AS label;
INSERT INTO bench_t_72 (id, payload) VALUES (17480, 'v17480');
SELECT `mysql_17481` FROM `tbl_31`;
SELECT nested FROM t WHERE id IN (17482, 17483, 17484);
WITH cte_17483 AS (SELECT 17483 AS n) SELECT n FROM cte_17483;
BEGIN; SELECT 17484; COMMIT;
BEGIN; SELECT 17485; COMMIT;
/* block header 17486 */
SELECT nested FROM t WHERE id IN (17487, 17488, 17489);
SELECT 17488 AS id, 'row_17488' AS label;
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT [bracket_17490] FROM [dbo].[tbl_10];
DELETE FROM bench_t_19 WHERE id = 3;
SELECT 17492 AS id, 'row_17492' AS label;
/* block header 17493 */
UPDATE bench_t_22 SET payload = 17494 WHERE id = 22;
SELECT `mysql_17495` FROM `tbl_45`;
$dz$ dollar body 17496 ; semicolon inside $dz$
SELECT `mysql_17497` FROM `tbl_47`;
SELECT `mysql_17498` FROM `tbl_48`;
/* block header 17499 */
/*
 * section 70
 * checksum 4748
 */
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
/* block header 17505 */
WITH cte_17506 AS (SELECT 17506 AS n) SELECT n FROM cte_17506;
$dz$ dollar body 17507 ; semicolon inside $dz$
DELETE FROM bench_t_4 WHERE id = 4;
BEGIN; SELECT 17509; COMMIT;
SELECT [bracket_17510] FROM [dbo].[tbl_30];
/* block header 17511 */
$dz$ dollar body 17512 ; semicolon inside $dz$
SELECT [bracket_17513] FROM [dbo].[tbl_33];
/* block header 17514 */
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
-- line 17516: deterministic comment
SELECT `mysql_17517` FROM `tbl_17`;
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
BEGIN; SELECT 17519; COMMIT;
/* block header 17520 */
UPDATE bench_t_49 SET payload = 17521 WHERE id = 17;
/* block header 17522 */
SELECT 17523 AS id, 'row_17523' AS label;
INSERT INTO bench_t_116 (id, payload) VALUES (17524, 'v17524');
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT 17526 AS id, 'row_17526' AS label;
DELETE FROM bench_t_23 WHERE id = 7;
SELECT 17528 AS id, 'row_17528' AS label;
DELETE FROM bench_t_25 WHERE id = 9;
DELETE FROM bench_t_26 WHERE id = 10;
INSERT INTO bench_t_123 (id, payload) VALUES (17531, 'v17531');
INSERT INTO bench_t_124 (id, payload) VALUES (17532, 'v17532');
$dz$ dollar body 17533 ; semicolon inside $dz$
SELECT `mysql_17534` FROM `tbl_34`;
-- line 17535: deterministic comment
WITH cte_17536 AS (SELECT 17536 AS n) SELECT n FROM cte_17536;
INSERT INTO bench_t_1 (id, payload) VALUES (17537, 'v17537');
$dz$ dollar body 17538 ; semicolon inside $dz$
SELECT [bracket_17539] FROM [dbo].[tbl_19];
SELECT nested FROM t WHERE id IN (17540, 17541, 17542);
SELECT [bracket_17541] FROM [dbo].[tbl_21];
SELECT 17542 AS id, 'row_17542' AS label;
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
BEGIN; SELECT 17544; COMMIT;
WITH cte_17545 AS (SELECT 17545 AS n) SELECT n FROM cte_17545;
# hash comment 17546
/* block header 17547 */
# hash comment 17548
SELECT 17549 AS id, 'row_17549' AS label;
-- line 17550: deterministic comment
INSERT INTO bench_t_15 (id, payload) VALUES (17551, 'v17551');
/* block header 17552 */
SELECT nested FROM t WHERE id IN (17553, 17554, 17555);
# hash comment 17554
SELECT * FROM "quoted_17555" WHERE col = E'esc\'17555';
BEGIN; SELECT 17556; COMMIT;
-- line 17557: deterministic comment
BEGIN; SELECT 17558; COMMIT;
SELECT 17559 AS id, 'row_17559' AS label;
UPDATE bench_t_24 SET payload = 17560 WHERE id = 24;
SELECT nested FROM t WHERE id IN (17561, 17562, 17563);
$dz$ dollar body 17562 ; semicolon inside $dz$
SELECT * FROM "quoted_17563" WHERE col = E'esc\'17563';
SELECT 17564 AS id, 'row_17564' AS label;
SELECT * FROM "quoted_17565" WHERE col = E'esc\'17565';
-- line 17566: deterministic comment
SELECT `mysql_17567` FROM `tbl_17`;
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT 17569 AS id, 'row_17569' AS label;
/* block header 17570 */
SELECT nested FROM t WHERE id IN (17571, 17572, 17573);
$dz$ dollar body 17572 ; semicolon inside $dz$
WITH cte_17573 AS (SELECT 17573 AS n) SELECT n FROM cte_17573;
SELECT `mysql_17574` FROM `tbl_24`;
WITH cte_17575 AS (SELECT 17575 AS n) SELECT n FROM cte_17575;
INSERT INTO bench_t_40 (id, payload) VALUES (17576, 'v17576');
DELETE FROM bench_t_9 WHERE id = 9;
SELECT [bracket_17578] FROM [dbo].[tbl_18];
DELETE FROM bench_t_11 WHERE id = 11;
INSERT INTO bench_t_44 (id, payload) VALUES (17580, 'v17580');
-- line 17581: deterministic comment
BEGIN; SELECT 17582; COMMIT;
INSERT INTO bench_t_47 (id, payload) VALUES (17583, 'v17583');
# hash comment 17584
SELECT `mysql_17585` FROM `tbl_35`;
DELETE FROM bench_t_18 WHERE id = 2;
INSERT INTO bench_t_51 (id, payload) VALUES (17587, 'v17587');
SELECT * FROM "quoted_17588" WHERE col = E'esc\'17588';
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
BEGIN; SELECT 17590; COMMIT;
UPDATE bench_t_55 SET payload = 17591 WHERE id = 23;
$dz$ dollar body 17592 ; semicolon inside $dz$
SELECT `mysql_17593` FROM `tbl_43`;
# hash comment 17594
SELECT * FROM "quoted_17595" WHERE col = E'esc\'17595';
# hash comment 17596
INSERT INTO bench_t_61 (id, payload) VALUES (17597, 'v17597');
DELETE FROM bench_t_30 WHERE id = 14;
UPDATE bench_t_63 SET payload = 17599 WHERE id = 31;
-- line 17600: deterministic comment
SELECT 17601 AS id, 'row_17601' AS label;
-- line 17602: deterministic comment
/* block header 17603 */
SELECT [bracket_17604] FROM [dbo].[tbl_4];
/* block header 17605 */
/* block header 17606 */
SELECT nested FROM t WHERE id IN (17607, 17608, 17609);
SELECT nested FROM t WHERE id IN (17608, 17609, 17610);
UPDATE bench_t_9 SET payload = 17609 WHERE id = 9;
SELECT * FROM "quoted_17610" WHERE col = E'esc\'17610';
INSERT INTO bench_t_75 (id, payload) VALUES (17611, 'O''Brien');
$dz$ dollar body 17612 ; semicolon inside $dz$
SELECT 17613 AS id, 'row_17613' AS label;
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT `mysql_17615` FROM `tbl_15`;
UPDATE bench_t_16 SET payload = 17616 WHERE id = 16;
SELECT [bracket_17617] FROM [dbo].[tbl_17];
UPDATE bench_t_18 SET payload = 17618 WHERE id = 18;
-- line 17619: deterministic comment
WITH cte_17620 AS (SELECT 17620 AS n) SELECT n FROM cte_17620;
SELECT nested FROM t WHERE id IN (17621, 17622, 17623);
DELETE FROM bench_t_22 WHERE id = 6;
SELECT `mysql_17623` FROM `tbl_23`;
BEGIN; SELECT 17624; COMMIT;
DELETE FROM bench_t_25 WHERE id = 9;
-- line 17626: deterministic comment
SELECT 17627 AS id, 'row_17627' AS label;
/* block header 17628 */
UPDATE bench_t_29 SET payload = 17629 WHERE id = 29;
SELECT nested FROM t WHERE id IN (17630, 17631, 17632);
$dz$ dollar body 17631 ; semicolon inside $dz$
-- line 17632: deterministic comment
SELECT `mysql_17633` FROM `tbl_33`;
-- line 17634: deterministic comment
DELETE FROM bench_t_3 WHERE id = 3;
DELETE FROM bench_t_4 WHERE id = 4;
INSERT INTO bench_t_101 (id, payload) VALUES (17637, 'v17637');
$dz$ dollar body 17638 ; semicolon inside $dz$
SELECT nested FROM t WHERE id IN (17639, 17640, 17641);
SELECT `mysql_17640` FROM `tbl_40`;
SELECT [bracket_17641] FROM [dbo].[tbl_1];
-- line 17642: deterministic comment
/* block header 17643 */
# hash comment 17644
DELETE FROM bench_t_13 WHERE id = 13;
SELECT `mysql_17646` FROM `tbl_46`;
UPDATE bench_t_47 SET payload = 17647 WHERE id = 15;
SELECT 17648 AS id, 'row_17648' AS label;
# hash comment 17649
-- line 17650: deterministic comment
$dz$ dollar body 17651 ; semicolon inside $dz$
SELECT `mysql_17652` FROM `tbl_2`;
SELECT * FROM "quoted_17653" WHERE col = E'esc\'17653';
$dz$ dollar body 17654 ; semicolon inside $dz$
$dz$ dollar body 17655 ; semicolon inside $dz$
BEGIN; SELECT 17656; COMMIT;
/* block header 17657 */
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
-- line 17659: deterministic comment
-- line 17660: deterministic comment
SELECT `mysql_17661` FROM `tbl_11`;
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT [bracket_17663] FROM [dbo].[tbl_23];
# hash comment 17664
SELECT nested FROM t WHERE id IN (17665, 17666, 17667);
# hash comment 17666
SELECT * FROM "quoted_17667" WHERE col = E'esc\'17667';
INSERT INTO bench_t_4 (id, payload) VALUES (17668, 'v17668');
SELECT * FROM "quoted_17669" WHERE col = E'esc\'17669';
SELECT nested FROM t WHERE id IN (17670, 17671, 17672);
SELECT [bracket_17671] FROM [dbo].[tbl_31];
SELECT * FROM "quoted_17672" WHERE col = E'esc\'17672';
SELECT 17673 AS id, 'row_17673' AS label;
SELECT * FROM "quoted_17674" WHERE col = E'esc\'17674';
SELECT * FROM "quoted_17675" WHERE col = E'esc\'17675';
SELECT * FROM "quoted_17676" WHERE col = E'esc\'17676';
DELETE FROM bench_t_13 WHERE id = 13;
BEGIN; SELECT 17678; COMMIT;
-- line 17679: deterministic comment
SELECT 17680 AS id, 'row_17680' AS label;
-- line 17681: deterministic comment
SELECT 17682 AS id, 'row_17682' AS label;
SELECT nested FROM t WHERE id IN (17683, 17684, 17685);
INSERT INTO bench_t_20 (id, payload) VALUES (17684, 'v17684');
# hash comment 17685
$dz$ dollar body 17686 ; semicolon inside $dz$
-- line 17687: deterministic comment
/* block header 17688 */
UPDATE bench_t_25 SET payload = 17689 WHERE id = 25;
SELECT 17690 AS id, 'row_17690' AS label;
SELECT * FROM "quoted_17691" WHERE col = E'esc\'17691';
$dz$ dollar body 17692 ; semicolon inside $dz$
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT nested FROM t WHERE id IN (17694, 17695, 17696);
UPDATE bench_t_31 SET payload = 17695 WHERE id = 31;
$dz$ dollar body 17696 ; semicolon inside $dz$
WITH cte_17697 AS (SELECT 17697 AS n) SELECT n FROM cte_17697;
UPDATE bench_t_34 SET payload = 17698 WHERE id = 2;
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
INSERT INTO bench_t_36 (id, payload) VALUES (17700, 'v17700');
$dz$ dollar body 17701 ; semicolon inside $dz$
-- line 17702: deterministic comment
UPDATE bench_t_39 SET payload = 17703 WHERE id = 7;
-- line 17704: deterministic comment
SELECT 17705 AS id, 'row_17705' AS label;
/* block header 17706 */
# hash comment 17707
SELECT `mysql_17708` FROM `tbl_8`;
WITH cte_17709 AS (SELECT 17709 AS n) SELECT n FROM cte_17709;
$dz$ dollar body 17710 ; semicolon inside $dz$
-- line 17711: deterministic comment
WITH cte_17712 AS (SELECT 17712 AS n) SELECT n FROM cte_17712;
BEGIN; SELECT 17713; COMMIT;
SELECT 17714 AS id, 'row_17714' AS label;
BEGIN; SELECT 17715; COMMIT;
# hash comment 17716
SELECT nested FROM t WHERE id IN (17717, 17718, 17719);
UPDATE bench_t_54 SET payload = 17718 WHERE id = 22;
$dz$ dollar body 17719 ; semicolon inside $dz$
SELECT 17720 AS id, 'row_17720' AS label;
SELECT [bracket_17721] FROM [dbo].[tbl_1];
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
UPDATE bench_t_59 SET payload = 17723 WHERE id = 27;
SELECT 17724 AS id, 'row_17724' AS label;
# hash comment 17725
SELECT nested FROM t WHERE id IN (17726, 17727, 17728);
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
DELETE FROM bench_t_0 WHERE id = 0;
SELECT [bracket_17729] FROM [dbo].[tbl_9];
/* block header 17730 */
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
# hash comment 17732
BEGIN; SELECT 17733; COMMIT;
SELECT [bracket_17734] FROM [dbo].[tbl_14];
SELECT nested FROM t WHERE id IN (17735, 17736, 17737);
DELETE FROM bench_t_8 WHERE id = 8;
$dz$ dollar body 17737 ; semicolon inside $dz$
DELETE FROM bench_t_10 WHERE id = 10;
-- line 17739: deterministic comment
BEGIN; SELECT 17740; COMMIT;
WITH cte_17741 AS (SELECT 17741 AS n) SELECT n FROM cte_17741;
SELECT nested FROM t WHERE id IN (17742, 17743, 17744);
SELECT `mysql_17743` FROM `tbl_43`;
SELECT [bracket_17744] FROM [dbo].[tbl_24];
SELECT `mysql_17745` FROM `tbl_45`;
SELECT `mysql_17746` FROM `tbl_46`;
/* block header 17747 */
BEGIN; SELECT 17748; COMMIT;
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
/*
 * section 71
 * checksum 6369
 */
SELECT 17750 AS id, 'row_17750' AS label;
/* block header 17755 */
SELECT [bracket_17756] FROM [dbo].[tbl_36];
WITH cte_17757 AS (SELECT 17757 AS n) SELECT n FROM cte_17757;
SELECT * FROM "quoted_17758" WHERE col = E'esc\'17758';
# hash comment 17759
SELECT 17760 AS id, 'row_17760' AS label;
DELETE FROM bench_t_1 WHERE id = 1;
SELECT nested FROM t WHERE id IN (17762, 17763, 17764);
SELECT 17763 AS id, 'row_17763' AS label;
/* block header 17764 */
UPDATE bench_t_37 SET payload = 17765 WHERE id = 5;
SELECT 17766 AS id, 'row_17766' AS label;
UPDATE bench_t_39 SET payload = 17767 WHERE id = 7;
-- line 17768: deterministic comment
/* block header 17769 */
WITH cte_17770 AS (SELECT 17770 AS n) SELECT n FROM cte_17770;
$dz$ dollar body 17771 ; semicolon inside $dz$
/* block header 17772 */
WITH cte_17773 AS (SELECT 17773 AS n) SELECT n FROM cte_17773;
INSERT INTO bench_t_110 (id, payload) VALUES (17774, 'v17774');
SELECT 17775 AS id, 'row_17775' AS label;
SELECT * FROM "quoted_17776" WHERE col = E'esc\'17776';
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
WITH cte_17778 AS (SELECT 17778 AS n) SELECT n FROM cte_17778;
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
WITH cte_17780 AS (SELECT 17780 AS n) SELECT n FROM cte_17780;
BEGIN; SELECT 17781; COMMIT;
-- line 17782: deterministic comment
BEGIN; SELECT 17783; COMMIT;
# hash comment 17784
SELECT `mysql_17785` FROM `tbl_35`;
BEGIN; SELECT 17786; COMMIT;
SELECT `mysql_17787` FROM `tbl_37`;
INSERT INTO bench_t_124 (id, payload) VALUES (17788, 'v17788');
$dz$ dollar body 17789 ; semicolon inside $dz$
SELECT 17790 AS id, 'row_17790' AS label;
DELETE FROM bench_t_31 WHERE id = 15;
BEGIN; SELECT 17792; COMMIT;
DELETE FROM bench_t_1 WHERE id = 1;
WITH cte_17794 AS (SELECT 17794 AS n) SELECT n FROM cte_17794;
SELECT * FROM "quoted_17795" WHERE col = E'esc\'17795';
INSERT INTO bench_t_4 (id, payload) VALUES (17796, 'v17796');
SELECT * FROM "quoted_17797" WHERE col = E'esc\'17797';
WITH cte_17798 AS (SELECT 17798 AS n) SELECT n FROM cte_17798;
SELECT [bracket_17799] FROM [dbo].[tbl_39];
$dz$ dollar body 17800 ; semicolon inside $dz$
# hash comment 17801
DELETE FROM bench_t_10 WHERE id = 10;
$dz$ dollar body 17803 ; semicolon inside $dz$
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
UPDATE bench_t_13 SET payload = 17805 WHERE id = 13;
SELECT * FROM "quoted_17806" WHERE col = E'esc\'17806';
/* block header 17807 */
-- line 17808: deterministic comment
/* block header 17809 */
WITH cte_17810 AS (SELECT 17810 AS n) SELECT n FROM cte_17810;
SELECT nested FROM t WHERE id IN (17811, 17812, 17813);
SELECT `mysql_17812` FROM `tbl_12`;
-- line 17813: deterministic comment
INSERT INTO bench_t_22 (id, payload) VALUES (17814, 'v17814');
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
# hash comment 17816
DELETE FROM bench_t_25 WHERE id = 9;
$dz$ dollar body 17818 ; semicolon inside $dz$
DELETE FROM bench_t_27 WHERE id = 11;
UPDATE bench_t_28 SET payload = 17820 WHERE id = 28;
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
DELETE FROM bench_t_30 WHERE id = 14;
UPDATE bench_t_31 SET payload = 17823 WHERE id = 31;
SELECT `mysql_17824` FROM `tbl_24`;
BEGIN; SELECT 17825; COMMIT;
SELECT 17826 AS id, 'row_17826' AS label;
UPDATE bench_t_35 SET payload = 17827 WHERE id = 3;
WITH cte_17828 AS (SELECT 17828 AS n) SELECT n FROM cte_17828;
SELECT `mysql_17829` FROM `tbl_29`;
$dz$ dollar body 17830 ; semicolon inside $dz$
BEGIN; SELECT 17831; COMMIT;
SELECT nested FROM t WHERE id IN (17832, 17833, 17834);
WITH cte_17833 AS (SELECT 17833 AS n) SELECT n FROM cte_17833;
SELECT 17834 AS id, 'row_17834' AS label;
DELETE FROM bench_t_11 WHERE id = 11;
SELECT 17836 AS id, 'row_17836' AS label;
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT * FROM "quoted_17838" WHERE col = E'esc\'17838';
$dz$ dollar body 17839 ; semicolon inside $dz$
/* block header 17840 */
$dz$ dollar body 17841 ; semicolon inside $dz$
WITH cte_17842 AS (SELECT 17842 AS n) SELECT n FROM cte_17842;
WITH cte_17843 AS (SELECT 17843 AS n) SELECT n FROM cte_17843;
SELECT 17844 AS id, 'row_17844' AS label;
UPDATE bench_t_53 SET payload = 17845 WHERE id = 21;
WITH cte_17846 AS (SELECT 17846 AS n) SELECT n FROM cte_17846;
# hash comment 17847
SELECT nested FROM t WHERE id IN (17848, 17849, 17850);
# hash comment 17849
SELECT * FROM "quoted_17850" WHERE col = E'esc\'17850';
# hash comment 17851
# hash comment 17852
$dz$ dollar body 17853 ; semicolon inside $dz$
/* block header 17854 */
SELECT * FROM "quoted_17855" WHERE col = E'esc\'17855';
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
# hash comment 17857
# hash comment 17858
SELECT nested FROM t WHERE id IN (17859, 17860, 17861);
DELETE FROM bench_t_4 WHERE id = 4;
UPDATE bench_t_5 SET payload = 17861 WHERE id = 5;
SELECT 17862 AS id, 'row_17862' AS label;
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT `mysql_17864` FROM `tbl_14`;
DELETE FROM bench_t_9 WHERE id = 9;
SELECT [bracket_17866] FROM [dbo].[tbl_26];
SELECT * FROM "quoted_17867" WHERE col = E'esc\'17867';
SELECT `mysql_17868` FROM `tbl_18`;
UPDATE bench_t_13 SET payload = 17869 WHERE id = 13;
SELECT `mysql_17870` FROM `tbl_20`;
/* block header 17871 */
$dz$ dollar body 17872 ; semicolon inside $dz$
SELECT `mysql_17873` FROM `tbl_23`;
WITH cte_17874 AS (SELECT 17874 AS n) SELECT n FROM cte_17874;
SELECT [bracket_17875] FROM [dbo].[tbl_35];
WITH cte_17876 AS (SELECT 17876 AS n) SELECT n FROM cte_17876;
UPDATE bench_t_21 SET payload = 17877 WHERE id = 21;
WITH cte_17878 AS (SELECT 17878 AS n) SELECT n FROM cte_17878;
SELECT `mysql_17879` FROM `tbl_29`;
SELECT * FROM "quoted_17880" WHERE col = E'esc\'17880';
UPDATE bench_t_25 SET payload = 17881 WHERE id = 25;
SELECT * FROM "quoted_17882" WHERE col = E'esc\'17882';
WITH cte_17883 AS (SELECT 17883 AS n) SELECT n FROM cte_17883;
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT * FROM "quoted_17885" WHERE col = E'esc\'17885';
SELECT nested FROM t WHERE id IN (17886, 17887, 17888);
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT [bracket_17888] FROM [dbo].[tbl_8];
SELECT [bracket_17889] FROM [dbo].[tbl_9];
WITH cte_17890 AS (SELECT 17890 AS n) SELECT n FROM cte_17890;
SELECT 17891 AS id, 'row_17891' AS label;
UPDATE bench_t_36 SET payload = 17892 WHERE id = 4;
WITH cte_17893 AS (SELECT 17893 AS n) SELECT n FROM cte_17893;
WITH cte_17894 AS (SELECT 17894 AS n) SELECT n FROM cte_17894;
WITH cte_17895 AS (SELECT 17895 AS n) SELECT n FROM cte_17895;
SELECT 17896 AS id, 'row_17896' AS label;
SELECT `mysql_17897` FROM `tbl_47`;
# hash comment 17898
SELECT [bracket_17899] FROM [dbo].[tbl_19];
WITH cte_17900 AS (SELECT 17900 AS n) SELECT n FROM cte_17900;
SELECT [bracket_17901] FROM [dbo].[tbl_21];
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT `mysql_17903` FROM `tbl_3`;
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
-- line 17905: deterministic comment
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT 17907 AS id, 'row_17907' AS label;
SELECT [bracket_17908] FROM [dbo].[tbl_28];
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT * FROM "quoted_17910" WHERE col = E'esc\'17910';
SELECT [bracket_17911] FROM [dbo].[tbl_31];
INSERT INTO bench_t_120 (id, payload) VALUES (17912, 'v17912');
DELETE FROM bench_t_25 WHERE id = 9;
INSERT INTO bench_t_122 (id, payload) VALUES (17914, 'v17914');
SELECT nested FROM t WHERE id IN (17915, 17916, 17917);
SELECT * FROM "quoted_17916" WHERE col = E'esc\'17916';
BEGIN; SELECT 17917; COMMIT;
# hash comment 17918
UPDATE bench_t_63 SET payload = 17919 WHERE id = 31;
SELECT [bracket_17920] FROM [dbo].[tbl_0];
DELETE FROM bench_t_1 WHERE id = 1;
SELECT `mysql_17922` FROM `tbl_22`;
BEGIN; SELECT 17923; COMMIT;
SELECT `mysql_17924` FROM `tbl_24`;
$dz$ dollar body 17925 ; semicolon inside $dz$
-- line 17926: deterministic comment
INSERT INTO bench_t_7 (id, payload) VALUES (17927, 'v17927');
SELECT nested FROM t WHERE id IN (17928, 17929, 17930);
INSERT INTO bench_t_9 (id, payload) VALUES (17929, 'v17929');
SELECT `mysql_17930` FROM `tbl_30`;
/* block header 17931 */
SELECT 17932 AS id, 'row_17932' AS label;
# hash comment 17933
/* block header 17934 */
SELECT [bracket_17935] FROM [dbo].[tbl_15];
BEGIN; SELECT 17936; COMMIT;
SELECT 17937 AS id, 'row_17937' AS label;
SELECT nested FROM t WHERE id IN (17938, 17939, 17940);
DELETE FROM bench_t_19 WHERE id = 3;
-- line 17940: deterministic comment
UPDATE bench_t_21 SET payload = 17941 WHERE id = 21;
SELECT `mysql_17942` FROM `tbl_42`;
SELECT `mysql_17943` FROM `tbl_43`;
UPDATE bench_t_24 SET payload = 17944 WHERE id = 24;
-- line 17945: deterministic comment
# hash comment 17946
-- line 17947: deterministic comment
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
DELETE FROM bench_t_29 WHERE id = 13;
# hash comment 17950
UPDATE bench_t_31 SET payload = 17951 WHERE id = 31;
/* block header 17952 */
UPDATE bench_t_33 SET payload = 17953 WHERE id = 1;
BEGIN; SELECT 17954; COMMIT;
BEGIN; SELECT 17955; COMMIT;
/* block header 17956 */
SELECT nested FROM t WHERE id IN (17957, 17958, 17959);
SELECT 17958 AS id, 'row_17958' AS label;
DELETE FROM bench_t_7 WHERE id = 7;
SELECT * FROM "quoted_17960" WHERE col = E'esc\'17960';
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT `mysql_17962` FROM `tbl_12`;
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
-- line 17966: deterministic comment
UPDATE bench_t_47 SET payload = 17967 WHERE id = 15;
DELETE FROM bench_t_16 WHERE id = 0;
DELETE FROM bench_t_17 WHERE id = 1;
SELECT nested FROM t WHERE id IN (17970, 17971, 17972);
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT [bracket_17972] FROM [dbo].[tbl_12];
SELECT [bracket_17973] FROM [dbo].[tbl_13];
$dz$ dollar body 17974 ; semicolon inside $dz$
SELECT * FROM "quoted_17975" WHERE col = E'esc\'17975';
SELECT 17976 AS id, 'row_17976' AS label;
UPDATE bench_t_57 SET payload = 17977 WHERE id = 25;
$dz$ dollar body 17978 ; semicolon inside $dz$
# hash comment 17979
INSERT INTO bench_t_60 (id, payload) VALUES (17980, 'v17980');
UPDATE bench_t_61 SET payload = 17981 WHERE id = 29;
SELECT `mysql_17982` FROM `tbl_32`;
SELECT 17983 AS id, 'row_17983' AS label;
WITH cte_17984 AS (SELECT 17984 AS n) SELECT n FROM cte_17984;
/* block header 17985 */
$dz$ dollar body 17986 ; semicolon inside $dz$
BEGIN; SELECT 17987; COMMIT;
SELECT [bracket_17988] FROM [dbo].[tbl_28];
SELECT `mysql_17989` FROM `tbl_39`;
SELECT nested FROM t WHERE id IN (17990, 17991, 17992);
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT * FROM "quoted_17992" WHERE col = E'esc\'17992';
WITH cte_17993 AS (SELECT 17993 AS n) SELECT n FROM cte_17993;
INSERT INTO bench_t_74 (id, payload) VALUES (17994, 'v17994');
SELECT [bracket_17995] FROM [dbo].[tbl_35];
/* block header 17996 */
SELECT [bracket_17997] FROM [dbo].[tbl_37];
$dz$ dollar body 17998 ; semicolon inside $dz$
BEGIN; SELECT 17999; COMMIT;
/*
 * section 72
 * checksum 143a
 */
SELECT nested FROM t WHERE id IN (18000, 18001, 18002);
WITH cte_18005 AS (SELECT 18005 AS n) SELECT n FROM cte_18005;
/* block header 18006 */
WITH cte_18007 AS (SELECT 18007 AS n) SELECT n FROM cte_18007;
INSERT INTO bench_t_88 (id, payload) VALUES (18008, 'v18008');
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
INSERT INTO bench_t_90 (id, payload) VALUES (18010, 'v18010');
UPDATE bench_t_27 SET payload = 18011 WHERE id = 27;
# hash comment 18012
WITH cte_18013 AS (SELECT 18013 AS n) SELECT n FROM cte_18013;
WITH cte_18014 AS (SELECT 18014 AS n) SELECT n FROM cte_18014;
BEGIN; SELECT 18015; COMMIT;
/* block header 18016 */
/* block header 18017 */
/* block header 18018 */
INSERT INTO bench_t_99 (id, payload) VALUES (18019, 'v18019');
SELECT [bracket_18020] FROM [dbo].[tbl_20];
/* block header 18021 */
UPDATE bench_t_38 SET payload = 18022 WHERE id = 6;
BEGIN; SELECT 18023; COMMIT;
DELETE FROM bench_t_8 WHERE id = 8;
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT nested FROM t WHERE id IN (18026, 18027, 18028);
/* block header 18027 */
# hash comment 18028
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
# hash comment 18030
SELECT [bracket_18031] FROM [dbo].[tbl_31];
-- line 18032: deterministic comment
BEGIN; SELECT 18033; COMMIT;
# hash comment 18034
DELETE FROM bench_t_19 WHERE id = 3;
SELECT nested FROM t WHERE id IN (18036, 18037, 18038);
/* block header 18037 */
# hash comment 18038
SELECT 18039 AS id, 'row_18039' AS label;
BEGIN; SELECT 18040; COMMIT;
SELECT 18041 AS id, 'row_18041' AS label;
/* block header 18042 */
BEGIN; SELECT 18043; COMMIT;
SELECT 18044 AS id, 'row_18044' AS label;
UPDATE bench_t_61 SET payload = 18045 WHERE id = 29;
SELECT 18046 AS id, 'row_18046' AS label;
WITH cte_18047 AS (SELECT 18047 AS n) SELECT n FROM cte_18047;
SELECT [bracket_18048] FROM [dbo].[tbl_8];
# hash comment 18049
/* block header 18050 */
# hash comment 18051
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT 18053 AS id, 'row_18053' AS label;
$dz$ dollar body 18054 ; semicolon inside $dz$
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT * FROM "quoted_18056" WHERE col = E'esc\'18056';
INSERT INTO bench_t_9 (id, payload) VALUES (18057, 'v18057');
WITH cte_18058 AS (SELECT 18058 AS n) SELECT n FROM cte_18058;
-- line 18059: deterministic comment
UPDATE bench_t_12 SET payload = 18060 WHERE id = 12;
UPDATE bench_t_13 SET payload = 18061 WHERE id = 13;
SELECT [bracket_18062] FROM [dbo].[tbl_22];
SELECT `mysql_18063` FROM `tbl_13`;
SELECT * FROM "quoted_18064" WHERE col = E'esc\'18064';
SELECT `mysql_18065` FROM `tbl_15`;
DELETE FROM bench_t_18 WHERE id = 2;
SELECT * FROM "quoted_18067" WHERE col = E'esc\'18067';
DELETE FROM bench_t_20 WHERE id = 4;
SELECT 18069 AS id, 'row_18069' AS label;
UPDATE bench_t_22 SET payload = 18070 WHERE id = 22;
INSERT INTO bench_t_23 (id, payload) VALUES (18071, 'v18071');
SELECT [bracket_18072] FROM [dbo].[tbl_32];
BEGIN; SELECT 18073; COMMIT;
DELETE FROM bench_t_26 WHERE id = 10;
SELECT [bracket_18075] FROM [dbo].[tbl_35];
UPDATE bench_t_28 SET payload = 18076 WHERE id = 28;
DELETE FROM bench_t_29 WHERE id = 13;
/* block header 18078 */
$dz$ dollar body 18079 ; semicolon inside $dz$
# hash comment 18080
INSERT INTO bench_t_33 (id, payload) VALUES (18081, 'v18081');
SELECT nested FROM t WHERE id IN (18082, 18083, 18084);
INSERT INTO bench_t_35 (id, payload) VALUES (18083, 'v18083');
SELECT nested FROM t WHERE id IN (18084, 18085, 18086);
BEGIN; SELECT 18085; COMMIT;
BEGIN; SELECT 18086; COMMIT;
SELECT * FROM "quoted_18087" WHERE col = E'esc\'18087';
$dz$ dollar body 18088 ; semicolon inside $dz$
BEGIN; SELECT 18089; COMMIT;
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
# hash comment 18091
SELECT * FROM "quoted_18092" WHERE col = E'esc\'18092';
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
# hash comment 18094
SELECT * FROM "quoted_18095" WHERE col = E'esc\'18095';
SELECT `mysql_18096` FROM `tbl_46`;
WITH cte_18097 AS (SELECT 18097 AS n) SELECT n FROM cte_18097;
/* block header 18098 */
SELECT * FROM "quoted_18099" WHERE col = E'esc\'18099';
DELETE FROM bench_t_20 WHERE id = 4;
/* block header 18101 */
$dz$ dollar body 18102 ; semicolon inside $dz$
SELECT 18103 AS id, 'row_18103' AS label;
$dz$ dollar body 18104 ; semicolon inside $dz$
SELECT nested FROM t WHERE id IN (18105, 18106, 18107);
SELECT [bracket_18106] FROM [dbo].[tbl_26];
WITH cte_18107 AS (SELECT 18107 AS n) SELECT n FROM cte_18107;
SELECT `mysql_18108` FROM `tbl_8`;
SELECT 18109 AS id, 'row_18109' AS label;
INSERT INTO bench_t_62 (id, payload) VALUES (18110, 'v18110');
$dz$ dollar body 18111 ; semicolon inside $dz$
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
INSERT INTO bench_t_66 (id, payload) VALUES (18114, 'v18114');
-- line 18115: deterministic comment
# hash comment 18116
INSERT INTO bench_t_69 (id, payload) VALUES (18117, 'O''Brien');
# hash comment 18118
SELECT * FROM "quoted_18119" WHERE col = E'esc\'18119';
BEGIN; SELECT 18120; COMMIT;
SELECT nested FROM t WHERE id IN (18121, 18122, 18123);
DELETE FROM bench_t_10 WHERE id = 10;
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
# hash comment 18124
UPDATE bench_t_13 SET payload = 18125 WHERE id = 13;
SELECT * FROM "quoted_18126" WHERE col = E'esc\'18126';
/* block header 18127 */
SELECT [bracket_18128] FROM [dbo].[tbl_8];
SELECT `mysql_18129` FROM `tbl_29`;
-- line 18130: deterministic comment
WITH cte_18131 AS (SELECT 18131 AS n) SELECT n FROM cte_18131;
SELECT * FROM "quoted_18132" WHERE col = E'esc\'18132';
SELECT 18133 AS id, 'row_18133' AS label;
INSERT INTO bench_t_86 (id, payload) VALUES (18134, 'v18134');
UPDATE bench_t_23 SET payload = 18135 WHERE id = 23;
SELECT nested FROM t WHERE id IN (18136, 18137, 18138);
WITH cte_18137 AS (SELECT 18137 AS n) SELECT n FROM cte_18137;
-- line 18138: deterministic comment
SELECT [bracket_18139] FROM [dbo].[tbl_19];
UPDATE bench_t_28 SET payload = 18140 WHERE id = 28;
UPDATE bench_t_29 SET payload = 18141 WHERE id = 29;
INSERT INTO bench_t_94 (id, payload) VALUES (18142, 'v18142');
-- line 18143: deterministic comment
WITH cte_18144 AS (SELECT 18144 AS n) SELECT n FROM cte_18144;
SELECT [bracket_18145] FROM [dbo].[tbl_25];
INSERT INTO bench_t_98 (id, payload) VALUES (18146, 'v18146');
$dz$ dollar body 18147 ; semicolon inside $dz$
UPDATE bench_t_36 SET payload = 18148 WHERE id = 4;
UPDATE bench_t_37 SET payload = 18149 WHERE id = 5;
-- line 18150: deterministic comment
SELECT `mysql_18151` FROM `tbl_1`;
SELECT * FROM "quoted_18152" WHERE col = E'esc\'18152';
/* block header 18153 */
SELECT * FROM "quoted_18154" WHERE col = E'esc\'18154';
/* block header 18155 */
SELECT * FROM "quoted_18156" WHERE col = E'esc\'18156';
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT 18158 AS id, 'row_18158' AS label;
/* block header 18159 */
SELECT `mysql_18160` FROM `tbl_10`;
$dz$ dollar body 18161 ; semicolon inside $dz$
WITH cte_18162 AS (SELECT 18162 AS n) SELECT n FROM cte_18162;
UPDATE bench_t_51 SET payload = 18163 WHERE id = 19;
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT `mysql_18165` FROM `tbl_15`;
# hash comment 18166
SELECT nested FROM t WHERE id IN (18167, 18168, 18169);
INSERT INTO bench_t_120 (id, payload) VALUES (18168, 'v18168');
UPDATE bench_t_57 SET payload = 18169 WHERE id = 25;
# hash comment 18170
DELETE FROM bench_t_27 WHERE id = 11;
SELECT * FROM "quoted_18172" WHERE col = E'esc\'18172';
SELECT 18173 AS id, 'row_18173' AS label;
INSERT INTO bench_t_126 (id, payload) VALUES (18174, 'v18174');
BEGIN; SELECT 18175; COMMIT;
DELETE FROM bench_t_0 WHERE id = 0;
SELECT `mysql_18177` FROM `tbl_27`;
SELECT 18178 AS id, 'row_18178' AS label;
DELETE FROM bench_t_3 WHERE id = 3;
SELECT `mysql_18180` FROM `tbl_30`;
SELECT [bracket_18181] FROM [dbo].[tbl_21];
# hash comment 18182
INSERT INTO bench_t_7 (id, payload) VALUES (18183, 'O''Brien');
-- line 18184: deterministic comment
-- line 18185: deterministic comment
SELECT `mysql_18186` FROM `tbl_36`;
SELECT nested FROM t WHERE id IN (18187, 18188, 18189);
DELETE FROM bench_t_12 WHERE id = 12;
/* block header 18189 */
SELECT `mysql_18190` FROM `tbl_40`;
SELECT * FROM "quoted_18191" WHERE col = E'esc\'18191';
SELECT `mysql_18192` FROM `tbl_42`;
SELECT `mysql_18193` FROM `tbl_43`;
DELETE FROM bench_t_18 WHERE id = 2;
INSERT INTO bench_t_19 (id, payload) VALUES (18195, 'v18195');
SELECT * FROM "quoted_18196" WHERE col = E'esc\'18196';
BEGIN; SELECT 18197; COMMIT;
$dz$ dollar body 18198 ; semicolon inside $dz$
SELECT nested FROM t WHERE id IN (18199, 18200, 18201);
DELETE FROM bench_t_24 WHERE id = 8;
SELECT nested FROM t WHERE id IN (18201, 18202, 18203);
SELECT * FROM "quoted_18202" WHERE col = E'esc\'18202';
SELECT nested FROM t WHERE id IN (18203, 18204, 18205);
BEGIN; SELECT 18204; COMMIT;
/* block header 18205 */
SELECT nested FROM t WHERE id IN (18206, 18207, 18208);
# hash comment 18207
SELECT 18208 AS id, 'row_18208' AS label;
-- line 18209: deterministic comment
INSERT INTO bench_t_34 (id, payload) VALUES (18210, 'v18210');
UPDATE bench_t_35 SET payload = 18211 WHERE id = 3;
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
UPDATE bench_t_37 SET payload = 18213 WHERE id = 5;
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT `mysql_18215` FROM `tbl_15`;
# hash comment 18216
-- line 18217: deterministic comment
WITH cte_18218 AS (SELECT 18218 AS n) SELECT n FROM cte_18218;
-- line 18219: deterministic comment
-- line 18220: deterministic comment
UPDATE bench_t_45 SET payload = 18221 WHERE id = 13;
/* block header 18222 */
$dz$ dollar body 18223 ; semicolon inside $dz$
SELECT * FROM "quoted_18224" WHERE col = E'esc\'18224';
DELETE FROM bench_t_17 WHERE id = 1;
-- line 18226: deterministic comment
INSERT INTO bench_t_51 (id, payload) VALUES (18227, 'O''Brien');
INSERT INTO bench_t_52 (id, payload) VALUES (18228, 'v18228');
SELECT 18229 AS id, 'row_18229' AS label;
/* block header 18230 */
INSERT INTO bench_t_55 (id, payload) VALUES (18231, 'v18231');
-- line 18232: deterministic comment
# hash comment 18233
/* block header 18234 */
SELECT 18235 AS id, 'row_18235' AS label;
/* block header 18236 */
$dz$ dollar body 18237 ; semicolon inside $dz$
SELECT 18238 AS id, 'row_18238' AS label;
SELECT nested FROM t WHERE id IN (18239, 18240, 18241);
# hash comment 18240
SELECT * FROM "quoted_18241" WHERE col = E'esc\'18241';
-- line 18242: deterministic comment
DELETE FROM bench_t_3 WHERE id = 3;
BEGIN; SELECT 18244; COMMIT;
# hash comment 18245
SELECT nested FROM t WHERE id IN (18246, 18247, 18248);
SELECT 18247 AS id, 'row_18247' AS label;
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
# hash comment 18249
/*
 * section 73
 * checksum abbe
 */
BEGIN; SELECT 18250; COMMIT;
SELECT 18255 AS id, 'row_18255' AS label;
SELECT 18256 AS id, 'row_18256' AS label;
SELECT `mysql_18257` FROM `tbl_7`;
SELECT nested FROM t WHERE id IN (18258, 18259, 18260);
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT nested FROM t WHERE id IN (18260, 18261, 18262);
-- line 18261: deterministic comment
SELECT nested FROM t WHERE id IN (18262, 18263, 18264);
WITH cte_18263 AS (SELECT 18263 AS n) SELECT n FROM cte_18263;
SELECT * FROM "quoted_18264" WHERE col = E'esc\'18264';
SELECT `mysql_18265` FROM `tbl_15`;
SELECT 18266 AS id, 'row_18266' AS label;
-- line 18267: deterministic comment
WITH cte_18268 AS (SELECT 18268 AS n) SELECT n FROM cte_18268;
BEGIN; SELECT 18269; COMMIT;
SELECT `mysql_18270` FROM `tbl_20`;
$dz$ dollar body 18271 ; semicolon inside $dz$
-- line 18272: deterministic comment
SELECT * FROM "quoted_18273" WHERE col = E'esc\'18273';
SELECT nested FROM t WHERE id IN (18274, 18275, 18276);
SELECT nested FROM t WHERE id IN (18275, 18276, 18277);
WITH cte_18276 AS (SELECT 18276 AS n) SELECT n FROM cte_18276;
SELECT 18277 AS id, 'row_18277' AS label;
UPDATE bench_t_38 SET payload = 18278 WHERE id = 6;
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
DELETE FROM bench_t_8 WHERE id = 8;
$dz$ dollar body 18281 ; semicolon inside $dz$
SELECT nested FROM t WHERE id IN (18282, 18283, 18284);
-- line 18283: deterministic comment
SELECT `mysql_18284` FROM `tbl_34`;
WITH cte_18285 AS (SELECT 18285 AS n) SELECT n FROM cte_18285;
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT [bracket_18287] FROM [dbo].[tbl_7];
/* block header 18288 */
# hash comment 18289
BEGIN; SELECT 18290; COMMIT;
INSERT INTO bench_t_115 (id, payload) VALUES (18291, 'v18291');
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT nested FROM t WHERE id IN (18293, 18294, 18295);
$dz$ dollar body 18294 ; semicolon inside $dz$
/* block header 18295 */
UPDATE bench_t_56 SET payload = 18296 WHERE id = 24;
# hash comment 18297
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
INSERT INTO bench_t_123 (id, payload) VALUES (18299, 'v18299');
INSERT INTO bench_t_124 (id, payload) VALUES (18300, 'v18300');
DELETE FROM bench_t_29 WHERE id = 13;
$dz$ dollar body 18302 ; semicolon inside $dz$
SELECT 18303 AS id, 'row_18303' AS label;
DELETE FROM bench_t_0 WHERE id = 0;
UPDATE bench_t_1 SET payload = 18305 WHERE id = 1;
SELECT 18306 AS id, 'row_18306' AS label;
/* block header 18307 */
DELETE FROM bench_t_4 WHERE id = 4;
UPDATE bench_t_5 SET payload = 18309 WHERE id = 5;
WITH cte_18310 AS (SELECT 18310 AS n) SELECT n FROM cte_18310;
WITH cte_18311 AS (SELECT 18311 AS n) SELECT n FROM cte_18311;
-- line 18312: deterministic comment
# hash comment 18313
SELECT 18314 AS id, 'row_18314' AS label;
SELECT [bracket_18315] FROM [dbo].[tbl_35];
-- line 18316: deterministic comment
BEGIN; SELECT 18317; COMMIT;
/* block header 18318 */
$dz$ dollar body 18319 ; semicolon inside $dz$
SELECT `mysql_18320` FROM `tbl_20`;
SELECT * FROM "quoted_18321" WHERE col = E'esc\'18321';
UPDATE bench_t_18 SET payload = 18322 WHERE id = 18;
DELETE FROM bench_t_19 WHERE id = 3;
/* block header 18324 */
SELECT 18325 AS id, 'row_18325' AS label;
$dz$ dollar body 18326 ; semicolon inside $dz$
# hash comment 18327
/* block header 18328 */
-- line 18329: deterministic comment
SELECT `mysql_18330` FROM `tbl_30`;
SELECT * FROM "quoted_18331" WHERE col = E'esc\'18331';
SELECT * FROM "quoted_18332" WHERE col = E'esc\'18332';
SELECT `mysql_18333` FROM `tbl_33`;
SELECT [bracket_18334] FROM [dbo].[tbl_14];
# hash comment 18335
SELECT 18336 AS id, 'row_18336' AS label;
INSERT INTO bench_t_33 (id, payload) VALUES (18337, 'O''Brien');
UPDATE bench_t_34 SET payload = 18338 WHERE id = 2;
-- line 18339: deterministic comment
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT * FROM "quoted_18341" WHERE col = E'esc\'18341';
BEGIN; SELECT 18342; COMMIT;
SELECT nested FROM t WHERE id IN (18343, 18344, 18345);
-- line 18344: deterministic comment
SELECT [bracket_18345] FROM [dbo].[tbl_25];
SELECT [bracket_18346] FROM [dbo].[tbl_26];
SELECT [bracket_18347] FROM [dbo].[tbl_27];
SELECT nested FROM t WHERE id IN (18348, 18349, 18350);
$dz$ dollar body 18349 ; semicolon inside $dz$
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT * FROM "quoted_18352" WHERE col = E'esc\'18352';
INSERT INTO bench_t_49 (id, payload) VALUES (18353, 'v18353');
WITH cte_18354 AS (SELECT 18354 AS n) SELECT n FROM cte_18354;
BEGIN; SELECT 18355; COMMIT;
-- line 18356: deterministic comment
SELECT * FROM "quoted_18357" WHERE col = E'esc\'18357';
BEGIN; SELECT 18358; COMMIT;
DELETE FROM bench_t_23 WHERE id = 7;
SELECT * FROM "quoted_18360" WHERE col = E'esc\'18360';
/* block header 18361 */
WITH cte_18362 AS (SELECT 18362 AS n) SELECT n FROM cte_18362;
SELECT `mysql_18363` FROM `tbl_13`;
SELECT [bracket_18364] FROM [dbo].[tbl_4];
INSERT INTO bench_t_61 (id, payload) VALUES (18365, 'v18365');
SELECT [bracket_18366] FROM [dbo].[tbl_6];
-- line 18367: deterministic comment
SELECT 18368 AS id, 'row_18368' AS label;
SELECT `mysql_18369` FROM `tbl_19`;
SELECT 18370 AS id, 'row_18370' AS label;
SELECT * FROM "quoted_18371" WHERE col = E'esc\'18371';
-- line 18372: deterministic comment
$dz$ dollar body 18373 ; semicolon inside $dz$
UPDATE bench_t_6 SET payload = 18374 WHERE id = 6;
SELECT * FROM "quoted_18375" WHERE col = E'esc\'18375';
SELECT * FROM "quoted_18376" WHERE col = E'esc\'18376';
SELECT * FROM "quoted_18377" WHERE col = E'esc\'18377';
$dz$ dollar body 18378 ; semicolon inside $dz$
SELECT 18379 AS id, 'row_18379' AS label;
SELECT * FROM "quoted_18380" WHERE col = E'esc\'18380';
BEGIN; SELECT 18381; COMMIT;
UPDATE bench_t_14 SET payload = 18382 WHERE id = 14;
WITH cte_18383 AS (SELECT 18383 AS n) SELECT n FROM cte_18383;
BEGIN; SELECT 18384; COMMIT;
DELETE FROM bench_t_17 WHERE id = 1;
WITH cte_18386 AS (SELECT 18386 AS n) SELECT n FROM cte_18386;
SELECT * FROM "quoted_18387" WHERE col = E'esc\'18387';
-- line 18388: deterministic comment
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
$dz$ dollar body 18390 ; semicolon inside $dz$
SELECT [bracket_18391] FROM [dbo].[tbl_31];
UPDATE bench_t_24 SET payload = 18392 WHERE id = 24;
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
INSERT INTO bench_t_91 (id, payload) VALUES (18395, 'v18395');
SELECT nested FROM t WHERE id IN (18396, 18397, 18398);
BEGIN; SELECT 18397; COMMIT;
SELECT * FROM "quoted_18398" WHERE col = E'esc\'18398';
SELECT * FROM "quoted_18399" WHERE col = E'esc\'18399';
# hash comment 18400
INSERT INTO bench_t_97 (id, payload) VALUES (18401, 'v18401');
SELECT * FROM "quoted_18402" WHERE col = E'esc\'18402';
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT nested FROM t WHERE id IN (18404, 18405, 18406);
-- line 18405: deterministic comment
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT 18407 AS id, 'row_18407' AS label;
UPDATE bench_t_40 SET payload = 18408 WHERE id = 8;
SELECT * FROM "quoted_18409" WHERE col = E'esc\'18409';
SELECT `mysql_18410` FROM `tbl_10`;
UPDATE bench_t_43 SET payload = 18411 WHERE id = 11;
SELECT [bracket_18412] FROM [dbo].[tbl_12];
SELECT 18413 AS id, 'row_18413' AS label;
# hash comment 18414
SELECT `mysql_18415` FROM `tbl_15`;
DELETE FROM bench_t_16 WHERE id = 0;
WITH cte_18417 AS (SELECT 18417 AS n) SELECT n FROM cte_18417;
DELETE FROM bench_t_18 WHERE id = 2;
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT * FROM "quoted_18420" WHERE col = E'esc\'18420';
-- line 18421: deterministic comment
UPDATE bench_t_54 SET payload = 18422 WHERE id = 22;
WITH cte_18423 AS (SELECT 18423 AS n) SELECT n FROM cte_18423;
/* block header 18424 */
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
UPDATE bench_t_58 SET payload = 18426 WHERE id = 26;
SELECT [bracket_18427] FROM [dbo].[tbl_27];
SELECT 18428 AS id, 'row_18428' AS label;
# hash comment 18429
SELECT 18430 AS id, 'row_18430' AS label;
SELECT nested FROM t WHERE id IN (18431, 18432, 18433);
-- line 18432: deterministic comment
SELECT `mysql_18433` FROM `tbl_33`;
BEGIN; SELECT 18434; COMMIT;
BEGIN; SELECT 18435; COMMIT;
# hash comment 18436
BEGIN; SELECT 18437; COMMIT;
UPDATE bench_t_6 SET payload = 18438 WHERE id = 6;
UPDATE bench_t_7 SET payload = 18439 WHERE id = 7;
SELECT [bracket_18440] FROM [dbo].[tbl_0];
BEGIN; SELECT 18441; COMMIT;
SELECT * FROM "quoted_18442" WHERE col = E'esc\'18442';
INSERT INTO bench_t_11 (id, payload) VALUES (18443, 'v18443');
/* block header 18444 */
$dz$ dollar body 18445 ; semicolon inside $dz$
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
DELETE FROM bench_t_15 WHERE id = 15;
SELECT `mysql_18448` FROM `tbl_48`;
SELECT nested FROM t WHERE id IN (18449, 18450, 18451);
SELECT * FROM "quoted_18450" WHERE col = E'esc\'18450';
SELECT * FROM "quoted_18451" WHERE col = E'esc\'18451';
SELECT * FROM "quoted_18452" WHERE col = E'esc\'18452';
SELECT * FROM "quoted_18453" WHERE col = E'esc\'18453';
SELECT `mysql_18454` FROM `tbl_4`;
BEGIN; SELECT 18455; COMMIT;
INSERT INTO bench_t_24 (id, payload) VALUES (18456, 'v18456');
SELECT * FROM "quoted_18457" WHERE col = E'esc\'18457';
SELECT nested FROM t WHERE id IN (18458, 18459, 18460);
DELETE FROM bench_t_27 WHERE id = 11;
SELECT nested FROM t WHERE id IN (18460, 18461, 18462);
$dz$ dollar body 18461 ; semicolon inside $dz$
# hash comment 18462
SELECT [bracket_18463] FROM [dbo].[tbl_23];
BEGIN; SELECT 18464; COMMIT;
UPDATE bench_t_33 SET payload = 18465 WHERE id = 1;
SELECT nested FROM t WHERE id IN (18466, 18467, 18468);
SELECT `mysql_18467` FROM `tbl_17`;
SELECT nested FROM t WHERE id IN (18468, 18469, 18470);
# hash comment 18469
$dz$ dollar body 18470 ; semicolon inside $dz$
DELETE FROM bench_t_7 WHERE id = 7;
INSERT INTO bench_t_40 (id, payload) VALUES (18472, 'v18472');
$dz$ dollar body 18473 ; semicolon inside $dz$
SELECT nested FROM t WHERE id IN (18474, 18475, 18476);
SELECT 18475 AS id, 'row_18475' AS label;
UPDATE bench_t_44 SET payload = 18476 WHERE id = 12;
BEGIN; SELECT 18477; COMMIT;
# hash comment 18478
BEGIN; SELECT 18479; COMMIT;
SELECT [bracket_18480] FROM [dbo].[tbl_0];
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
-- line 18482: deterministic comment
DELETE FROM bench_t_19 WHERE id = 3;
WITH cte_18484 AS (SELECT 18484 AS n) SELECT n FROM cte_18484;
/* block header 18485 */
$dz$ dollar body 18486 ; semicolon inside $dz$
DELETE FROM bench_t_23 WHERE id = 7;
SELECT [bracket_18488] FROM [dbo].[tbl_8];
SELECT `mysql_18489` FROM `tbl_39`;
/* block header 18490 */
SELECT [bracket_18491] FROM [dbo].[tbl_11];
DELETE FROM bench_t_28 WHERE id = 12;
$dz$ dollar body 18493 ; semicolon inside $dz$
DELETE FROM bench_t_30 WHERE id = 14;
SELECT nested FROM t WHERE id IN (18495, 18496, 18497);
SELECT * FROM "quoted_18496" WHERE col = E'esc\'18496';
SELECT `mysql_18497` FROM `tbl_47`;
-- line 18498: deterministic comment
DELETE FROM bench_t_3 WHERE id = 3;
/*
 * section 74
 * checksum cbde
 */
-- line 18500: deterministic comment
INSERT INTO bench_t_73 (id, payload) VALUES (18505, 'v18505');
DELETE FROM bench_t_10 WHERE id = 10;
/* block header 18507 */
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
UPDATE bench_t_13 SET payload = 18509 WHERE id = 13;
-- line 18510: deterministic comment
SELECT * FROM "quoted_18511" WHERE col = E'esc\'18511';
SELECT nested FROM t WHERE id IN (18512, 18513, 18514);
/* block header 18513 */
BEGIN; SELECT 18514; COMMIT;
INSERT INTO bench_t_83 (id, payload) VALUES (18515, 'v18515');
WITH cte_18516 AS (SELECT 18516 AS n) SELECT n FROM cte_18516;
-- line 18517: deterministic comment
-- line 18518: deterministic comment
WITH cte_18519 AS (SELECT 18519 AS n) SELECT n FROM cte_18519;
# hash comment 18520
BEGIN; SELECT 18521; COMMIT;
SELECT * FROM "quoted_18522" WHERE col = E'esc\'18522';
SELECT * FROM "quoted_18523" WHERE col = E'esc\'18523';
BEGIN; SELECT 18524; COMMIT;
BEGIN; SELECT 18525; COMMIT;
SELECT 18526 AS id, 'row_18526' AS label;
WITH cte_18527 AS (SELECT 18527 AS n) SELECT n FROM cte_18527;
SELECT nested FROM t WHERE id IN (18528, 18529, 18530);
$dz$ dollar body 18529 ; semicolon inside $dz$
SELECT [bracket_18530] FROM [dbo].[tbl_10];
SELECT `mysql_18531` FROM `tbl_31`;
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
$dz$ dollar body 18533 ; semicolon inside $dz$
-- line 18534: deterministic comment
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
/* block header 18536 */
SELECT [bracket_18537] FROM [dbo].[tbl_17];
/* block header 18538 */
INSERT INTO bench_t_107 (id, payload) VALUES (18539, 'v18539');
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
UPDATE bench_t_45 SET payload = 18541 WHERE id = 13;
UPDATE bench_t_46 SET payload = 18542 WHERE id = 14;
-- line 18543: deterministic comment
/* block header 18544 */
SELECT 18545 AS id, 'row_18545' AS label;
SELECT `mysql_18546` FROM `tbl_46`;
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
INSERT INTO bench_t_116 (id, payload) VALUES (18548, 'v18548');
# hash comment 18549
SELECT * FROM "quoted_18550" WHERE col = E'esc\'18550';
SELECT [bracket_18551] FROM [dbo].[tbl_31];
# hash comment 18552
SELECT * FROM "quoted_18553" WHERE col = E'esc\'18553';
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
DELETE FROM bench_t_28 WHERE id = 12;
/* block header 18557 */
# hash comment 18558
/* block header 18559 */
/* block header 18560 */
-- line 18561: deterministic comment
SELECT [bracket_18562] FROM [dbo].[tbl_2];
SELECT nested FROM t WHERE id IN (18563, 18564, 18565);
# hash comment 18564
$dz$ dollar body 18565 ; semicolon inside $dz$
SELECT [bracket_18566] FROM [dbo].[tbl_6];
DELETE FROM bench_t_7 WHERE id = 7;
INSERT INTO bench_t_8 (id, payload) VALUES (18568, 'O''Brien');
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
UPDATE bench_t_10 SET payload = 18570 WHERE id = 10;
UPDATE bench_t_11 SET payload = 18571 WHERE id = 11;
SELECT `mysql_18572` FROM `tbl_22`;
SELECT `mysql_18573` FROM `tbl_23`;
-- line 18574: deterministic comment
BEGIN; SELECT 18575; COMMIT;
SELECT nested FROM t WHERE id IN (18576, 18577, 18578);
DELETE FROM bench_t_17 WHERE id = 1;
SELECT nested FROM t WHERE id IN (18578, 18579, 18580);
$dz$ dollar body 18579 ; semicolon inside $dz$
$dz$ dollar body 18580 ; semicolon inside $dz$
UPDATE bench_t_21 SET payload = 18581 WHERE id = 21;
SELECT 18582 AS id, 'row_18582' AS label;
INSERT INTO bench_t_23 (id, payload) VALUES (18583, 'v18583');
SELECT nested FROM t WHERE id IN (18584, 18585, 18586);
BEGIN; SELECT 18585; COMMIT;
SELECT `mysql_18586` FROM `tbl_36`;
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
$dz$ dollar body 18588 ; semicolon inside $dz$
SELECT [bracket_18589] FROM [dbo].[tbl_29];
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT `mysql_18591` FROM `tbl_41`;
$dz$ dollar body 18592 ; semicolon inside $dz$
SELECT 18593 AS id, 'row_18593' AS label;
-- line 18594: deterministic comment
$dz$ dollar body 18595 ; semicolon inside $dz$
/* block header 18596 */
$dz$ dollar body 18597 ; semicolon inside $dz$
DELETE FROM bench_t_6 WHERE id = 6;
# hash comment 18599
SELECT [bracket_18600] FROM [dbo].[tbl_0];
SELECT nested FROM t WHERE id IN (18601, 18602, 18603);
SELECT 18602 AS id, 'row_18602' AS label;
BEGIN; SELECT 18603; COMMIT;
SELECT * FROM "quoted_18604" WHERE col = E'esc\'18604';
SELECT `mysql_18605` FROM `tbl_5`;
SELECT * FROM "quoted_18606" WHERE col = E'esc\'18606';
-- line 18607: deterministic comment
SELECT 18608 AS id, 'row_18608' AS label;
-- line 18609: deterministic comment
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
-- line 18611: deterministic comment
/* block header 18612 */
SELECT 18613 AS id, 'row_18613' AS label;
SELECT 18614 AS id, 'row_18614' AS label;
UPDATE bench_t_55 SET payload = 18615 WHERE id = 23;
BEGIN; SELECT 18616; COMMIT;
UPDATE bench_t_57 SET payload = 18617 WHERE id = 25;
$dz$ dollar body 18618 ; semicolon inside $dz$
INSERT INTO bench_t_59 (id, payload) VALUES (18619, 'v18619');
SELECT nested FROM t WHERE id IN (18620, 18621, 18622);
UPDATE bench_t_61 SET payload = 18621 WHERE id = 29;
BEGIN; SELECT 18622; COMMIT;
SELECT `mysql_18623` FROM `tbl_23`;
UPDATE bench_t_0 SET payload = 18624 WHERE id = 0;
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT `mysql_18626` FROM `tbl_26`;
UPDATE bench_t_3 SET payload = 18627 WHERE id = 3;
/* block header 18628 */
$dz$ dollar body 18629 ; semicolon inside $dz$
SELECT [bracket_18630] FROM [dbo].[tbl_30];
# hash comment 18631
UPDATE bench_t_8 SET payload = 18632 WHERE id = 8;
# hash comment 18633
WITH cte_18634 AS (SELECT 18634 AS n) SELECT n FROM cte_18634;
/* block header 18635 */
/* block header 18636 */
UPDATE bench_t_13 SET payload = 18637 WHERE id = 13;
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT `mysql_18639` FROM `tbl_39`;
/* block header 18640 */
SELECT `mysql_18641` FROM `tbl_41`;
-- line 18642: deterministic comment
DELETE FROM bench_t_19 WHERE id = 3;
BEGIN; SELECT 18644; COMMIT;
DELETE FROM bench_t_21 WHERE id = 5;
SELECT nested FROM t WHERE id IN (18646, 18647, 18648);
SELECT nested FROM t WHERE id IN (18647, 18648, 18649);
SELECT nested FROM t WHERE id IN (18648, 18649, 18650);
# hash comment 18649
SELECT nested FROM t WHERE id IN (18650, 18651, 18652);
UPDATE bench_t_27 SET payload = 18651 WHERE id = 27;
# hash comment 18652
DELETE FROM bench_t_29 WHERE id = 13;
INSERT INTO bench_t_94 (id, payload) VALUES (18654, 'v18654');
/* block header 18655 */
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT * FROM "quoted_18657" WHERE col = E'esc\'18657';
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT nested FROM t WHERE id IN (18659, 18660, 18661);
/* block header 18660 */
UPDATE bench_t_37 SET payload = 18661 WHERE id = 5;
WITH cte_18662 AS (SELECT 18662 AS n) SELECT n FROM cte_18662;
WITH cte_18663 AS (SELECT 18663 AS n) SELECT n FROM cte_18663;
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
INSERT INTO bench_t_105 (id, payload) VALUES (18665, 'v18665');
-- line 18666: deterministic comment
DELETE FROM bench_t_11 WHERE id = 11;
# hash comment 18668
SELECT [bracket_18669] FROM [dbo].[tbl_29];
UPDATE bench_t_46 SET payload = 18670 WHERE id = 14;
WITH cte_18671 AS (SELECT 18671 AS n) SELECT n FROM cte_18671;
UPDATE bench_t_48 SET payload = 18672 WHERE id = 16;
SELECT * FROM "quoted_18673" WHERE col = E'esc\'18673';
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
$dz$ dollar body 18675 ; semicolon inside $dz$
BEGIN; SELECT 18676; COMMIT;
$dz$ dollar body 18677 ; semicolon inside $dz$
SELECT [bracket_18678] FROM [dbo].[tbl_38];
/* block header 18679 */
BEGIN; SELECT 18680; COMMIT;
-- line 18681: deterministic comment
SELECT nested FROM t WHERE id IN (18682, 18683, 18684);
WITH cte_18683 AS (SELECT 18683 AS n) SELECT n FROM cte_18683;
SELECT `mysql_18684` FROM `tbl_34`;
BEGIN; SELECT 18685; COMMIT;
WITH cte_18686 AS (SELECT 18686 AS n) SELECT n FROM cte_18686;
$dz$ dollar body 18687 ; semicolon inside $dz$
INSERT INTO bench_t_0 (id, payload) VALUES (18688, 'v18688');
WITH cte_18689 AS (SELECT 18689 AS n) SELECT n FROM cte_18689;
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
# hash comment 18691
DELETE FROM bench_t_4 WHERE id = 4;
SELECT nested FROM t WHERE id IN (18693, 18694, 18695);
# hash comment 18694
SELECT [bracket_18695] FROM [dbo].[tbl_15];
SELECT * FROM "quoted_18696" WHERE col = E'esc\'18696';
$dz$ dollar body 18697 ; semicolon inside $dz$
WITH cte_18698 AS (SELECT 18698 AS n) SELECT n FROM cte_18698;
UPDATE bench_t_11 SET payload = 18699 WHERE id = 11;
SELECT nested FROM t WHERE id IN (18700, 18701, 18702);
DELETE FROM bench_t_13 WHERE id = 13;
/* block header 18702 */
INSERT INTO bench_t_15 (id, payload) VALUES (18703, 'v18703');
INSERT INTO bench_t_16 (id, payload) VALUES (18704, 'v18704');
# hash comment 18705
DELETE FROM bench_t_18 WHERE id = 2;
INSERT INTO bench_t_19 (id, payload) VALUES (18707, 'v18707');
SELECT 18708 AS id, 'row_18708' AS label;
DELETE FROM bench_t_21 WHERE id = 5;
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
$dz$ dollar body 18711 ; semicolon inside $dz$
UPDATE bench_t_24 SET payload = 18712 WHERE id = 24;
-- line 18713: deterministic comment
# hash comment 18714
DELETE FROM bench_t_27 WHERE id = 11;
DELETE FROM bench_t_28 WHERE id = 12;
DELETE FROM bench_t_29 WHERE id = 13;
DELETE FROM bench_t_30 WHERE id = 14;
SELECT * FROM "quoted_18719" WHERE col = E'esc\'18719';
SELECT [bracket_18720] FROM [dbo].[tbl_0];
SELECT `mysql_18721` FROM `tbl_21`;
SELECT nested FROM t WHERE id IN (18722, 18723, 18724);
BEGIN; SELECT 18723; COMMIT;
# hash comment 18724
UPDATE bench_t_37 SET payload = 18725 WHERE id = 5;
SELECT [bracket_18726] FROM [dbo].[tbl_6];
/* block header 18727 */
# hash comment 18728
DELETE FROM bench_t_9 WHERE id = 9;
SELECT 18730 AS id, 'row_18730' AS label;
SELECT * FROM "quoted_18731" WHERE col = E'esc\'18731';
INSERT INTO bench_t_44 (id, payload) VALUES (18732, 'v18732');
WITH cte_18733 AS (SELECT 18733 AS n) SELECT n FROM cte_18733;
# hash comment 18734
# hash comment 18735
SELECT [bracket_18736] FROM [dbo].[tbl_16];
/* block header 18737 */
SELECT [bracket_18738] FROM [dbo].[tbl_18];
SELECT 18739 AS id, 'row_18739' AS label;
SELECT 18740 AS id, 'row_18740' AS label;
DELETE FROM bench_t_21 WHERE id = 5;
SELECT `mysql_18742` FROM `tbl_42`;
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT * FROM "quoted_18744" WHERE col = E'esc\'18744';
SELECT nested FROM t WHERE id IN (18745, 18746, 18747);
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
-- line 18747: deterministic comment
INSERT INTO bench_t_60 (id, payload) VALUES (18748, 'v18748');
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
/*
 * section 75
 * checksum 7feb
 */
INSERT INTO bench_t_62 (id, payload) VALUES (18750, 'v18750');
UPDATE bench_t_3 SET payload = 18755 WHERE id = 3;
SELECT 18756 AS id, 'row_18756' AS label;
$dz$ dollar body 18757 ; semicolon inside $dz$
WITH cte_18758 AS (SELECT 18758 AS n) SELECT n FROM cte_18758;
SELECT [bracket_18759] FROM [dbo].[tbl_39];
# hash comment 18760
BEGIN; SELECT 18761; COMMIT;
BEGIN; SELECT 18762; COMMIT;
WITH cte_18763 AS (SELECT 18763 AS n) SELECT n FROM cte_18763;
SELECT [bracket_18764] FROM [dbo].[tbl_4];
SELECT `mysql_18765` FROM `tbl_15`;
SELECT * FROM "quoted_18766" WHERE col = E'esc\'18766';
SELECT 18767 AS id, 'row_18767' AS label;
$dz$ dollar body 18768 ; semicolon inside $dz$
SELECT * FROM "quoted_18769" WHERE col = E'esc\'18769';
SELECT * FROM "quoted_18770" WHERE col = E'esc\'18770';
/* block header 18771 */
-- line 18772: deterministic comment
INSERT INTO bench_t_85 (id, payload) VALUES (18773, 'v18773');
SELECT [bracket_18774] FROM [dbo].[tbl_14];
WITH cte_18775 AS (SELECT 18775 AS n) SELECT n FROM cte_18775;
BEGIN; SELECT 18776; COMMIT;
SELECT * FROM "quoted_18777" WHERE col = E'esc\'18777';
WITH cte_18778 AS (SELECT 18778 AS n) SELECT n FROM cte_18778;
# hash comment 18779
DELETE FROM bench_t_28 WHERE id = 12;
SELECT `mysql_18781` FROM `tbl_31`;
SELECT [bracket_18782] FROM [dbo].[tbl_22];
INSERT INTO bench_t_95 (id, payload) VALUES (18783, 'v18783');
$dz$ dollar body 18784 ; semicolon inside $dz$
DELETE FROM bench_t_1 WHERE id = 1;
SELECT [bracket_18786] FROM [dbo].[tbl_26];
SELECT `mysql_18787` FROM `tbl_37`;
SELECT 18788 AS id, 'row_18788' AS label;
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT nested FROM t WHERE id IN (18790, 18791, 18792);
SELECT * FROM "quoted_18791" WHERE col = E'esc\'18791';
SELECT * FROM "quoted_18792" WHERE col = E'esc\'18792';
$dz$ dollar body 18793 ; semicolon inside $dz$
INSERT INTO bench_t_106 (id, payload) VALUES (18794, 'v18794');
BEGIN; SELECT 18795; COMMIT;
UPDATE bench_t_44 SET payload = 18796 WHERE id = 12;
WITH cte_18797 AS (SELECT 18797 AS n) SELECT n FROM cte_18797;
WITH cte_18798 AS (SELECT 18798 AS n) SELECT n FROM cte_18798;
SELECT 18799 AS id, 'row_18799' AS label;
SELECT * FROM "quoted_18800" WHERE col = E'esc\'18800';
UPDATE bench_t_49 SET payload = 18801 WHERE id = 17;
UPDATE bench_t_50 SET payload = 18802 WHERE id = 18;
SELECT nested FROM t WHERE id IN (18803, 18804, 18805);
SELECT `mysql_18804` FROM `tbl_4`;
$dz$ dollar body 18805 ; semicolon inside $dz$
UPDATE bench_t_54 SET payload = 18806 WHERE id = 22;
SELECT `mysql_18807` FROM `tbl_7`;
DELETE FROM bench_t_24 WHERE id = 8;
$dz$ dollar body 18809 ; semicolon inside $dz$
SELECT 18810 AS id, 'row_18810' AS label;
/* block header 18811 */
/* block header 18812 */
$dz$ dollar body 18813 ; semicolon inside $dz$
SELECT `mysql_18814` FROM `tbl_14`;
-- line 18815: deterministic comment
SELECT `mysql_18816` FROM `tbl_16`;
# hash comment 18817
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
-- line 18819: deterministic comment
SELECT `mysql_18820` FROM `tbl_20`;
SELECT * FROM "quoted_18821" WHERE col = E'esc\'18821';
WITH cte_18822 AS (SELECT 18822 AS n) SELECT n FROM cte_18822;
/* block header 18823 */
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
$dz$ dollar body 18827 ; semicolon inside $dz$
SELECT * FROM "quoted_18828" WHERE col = E'esc\'18828';
SELECT [bracket_18829] FROM [dbo].[tbl_29];
$dz$ dollar body 18830 ; semicolon inside $dz$
DELETE FROM bench_t_15 WHERE id = 15;
UPDATE bench_t_16 SET payload = 18832 WHERE id = 16;
# hash comment 18833
WITH cte_18834 AS (SELECT 18834 AS n) SELECT n FROM cte_18834;
SELECT 18835 AS id, 'row_18835' AS label;
SELECT nested FROM t WHERE id IN (18836, 18837, 18838);
SELECT 18837 AS id, 'row_18837' AS label;
$dz$ dollar body 18838 ; semicolon inside $dz$
# hash comment 18839
BEGIN; SELECT 18840; COMMIT;
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT [bracket_18842] FROM [dbo].[tbl_2];
DELETE FROM bench_t_27 WHERE id = 11;
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT `mysql_18845` FROM `tbl_45`;
$dz$ dollar body 18846 ; semicolon inside $dz$
SELECT [bracket_18847] FROM [dbo].[tbl_7];
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
WITH cte_18849 AS (SELECT 18849 AS n) SELECT n FROM cte_18849;
BEGIN; SELECT 18850; COMMIT;
DELETE FROM bench_t_3 WHERE id = 3;
SELECT * FROM "quoted_18852" WHERE col = E'esc\'18852';
BEGIN; SELECT 18853; COMMIT;
# hash comment 18854
SELECT * FROM "quoted_18855" WHERE col = E'esc\'18855';
SELECT nested FROM t WHERE id IN (18856, 18857, 18858);
SELECT nested FROM t WHERE id IN (18857, 18858, 18859);
INSERT INTO bench_t_42 (id, payload) VALUES (18858, 'v18858');
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT `mysql_18860` FROM `tbl_10`;
SELECT * FROM "quoted_18861" WHERE col = E'esc\'18861';
WITH cte_18862 AS (SELECT 18862 AS n) SELECT n FROM cte_18862;
UPDATE bench_t_47 SET payload = 18863 WHERE id = 15;
SELECT [bracket_18864] FROM [dbo].[tbl_24];
UPDATE bench_t_49 SET payload = 18865 WHERE id = 17;
DELETE FROM bench_t_18 WHERE id = 2;
WITH cte_18867 AS (SELECT 18867 AS n) SELECT n FROM cte_18867;
BEGIN; SELECT 18868; COMMIT;
SELECT [bracket_18869] FROM [dbo].[tbl_29];
INSERT INTO bench_t_54 (id, payload) VALUES (18870, 'v18870');
SELECT 18871 AS id, 'row_18871' AS label;
SELECT 18872 AS id, 'row_18872' AS label;
BEGIN; SELECT 18873; COMMIT;
INSERT INTO bench_t_58 (id, payload) VALUES (18874, 'v18874');
UPDATE bench_t_59 SET payload = 18875 WHERE id = 27;
SELECT `mysql_18876` FROM `tbl_26`;
INSERT INTO bench_t_61 (id, payload) VALUES (18877, 'v18877');
SELECT [bracket_18878] FROM [dbo].[tbl_38];
UPDATE bench_t_63 SET payload = 18879 WHERE id = 31;
/* block header 18880 */
-- line 18881: deterministic comment
SELECT [bracket_18882] FROM [dbo].[tbl_2];
SELECT * FROM "quoted_18883" WHERE col = E'esc\'18883';
$dz$ dollar body 18884 ; semicolon inside $dz$
BEGIN; SELECT 18885; COMMIT;
SELECT `mysql_18886` FROM `tbl_36`;
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT `mysql_18888` FROM `tbl_38`;
DELETE FROM bench_t_9 WHERE id = 9;
SELECT nested FROM t WHERE id IN (18890, 18891, 18892);
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
-- line 18892: deterministic comment
WITH cte_18893 AS (SELECT 18893 AS n) SELECT n FROM cte_18893;
/* block header 18894 */
WITH cte_18895 AS (SELECT 18895 AS n) SELECT n FROM cte_18895;
$dz$ dollar body 18896 ; semicolon inside $dz$
-- line 18897: deterministic comment
SELECT nested FROM t WHERE id IN (18898, 18899, 18900);
SELECT * FROM "quoted_18899" WHERE col = E'esc\'18899';
SELECT [bracket_18900] FROM [dbo].[tbl_20];
# hash comment 18901
INSERT INTO bench_t_86 (id, payload) VALUES (18902, 'v18902');
$dz$ dollar body 18903 ; semicolon inside $dz$
SELECT * FROM "quoted_18904" WHERE col = E'esc\'18904';
BEGIN; SELECT 18905; COMMIT;
BEGIN; SELECT 18906; COMMIT;
SELECT 18907 AS id, 'row_18907' AS label;
SELECT [bracket_18908] FROM [dbo].[tbl_28];
$dz$ dollar body 18909 ; semicolon inside $dz$
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT nested FROM t WHERE id IN (18911, 18912, 18913);
SELECT * FROM "quoted_18912" WHERE col = E'esc\'18912';
-- line 18913: deterministic comment
INSERT INTO bench_t_98 (id, payload) VALUES (18914, 'v18914');
-- line 18915: deterministic comment
BEGIN; SELECT 18916; COMMIT;
UPDATE bench_t_37 SET payload = 18917 WHERE id = 5;
UPDATE bench_t_38 SET payload = 18918 WHERE id = 6;
$dz$ dollar body 18919 ; semicolon inside $dz$
BEGIN; SELECT 18920; COMMIT;
WITH cte_18921 AS (SELECT 18921 AS n) SELECT n FROM cte_18921;
# hash comment 18922
BEGIN; SELECT 18923; COMMIT;
DELETE FROM bench_t_12 WHERE id = 12;
INSERT INTO bench_t_109 (id, payload) VALUES (18925, 'v18925');
DELETE FROM bench_t_14 WHERE id = 14;
# hash comment 18927
-- line 18928: deterministic comment
SELECT `mysql_18929` FROM `tbl_29`;
SELECT * FROM "quoted_18930" WHERE col = E'esc\'18930';
SELECT `mysql_18931` FROM `tbl_31`;
/* block header 18932 */
UPDATE bench_t_53 SET payload = 18933 WHERE id = 21;
INSERT INTO bench_t_118 (id, payload) VALUES (18934, 'v18934');
DELETE FROM bench_t_23 WHERE id = 7;
UPDATE bench_t_56 SET payload = 18936 WHERE id = 24;
SELECT nested FROM t WHERE id IN (18937, 18938, 18939);
SELECT `mysql_18938` FROM `tbl_38`;
SELECT 18939 AS id, 'row_18939' AS label;
# hash comment 18940
SELECT * FROM "quoted_18941" WHERE col = E'esc\'18941';
# hash comment 18942
$dz$ dollar body 18943 ; semicolon inside $dz$
UPDATE bench_t_0 SET payload = 18944 WHERE id = 0;
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT [bracket_18947] FROM [dbo].[tbl_27];
$dz$ dollar body 18948 ; semicolon inside $dz$
/* block header 18949 */
UPDATE bench_t_6 SET payload = 18950 WHERE id = 6;
UPDATE bench_t_7 SET payload = 18951 WHERE id = 7;
-- line 18952: deterministic comment
SELECT 18953 AS id, 'row_18953' AS label;
SELECT [bracket_18954] FROM [dbo].[tbl_34];
-- line 18955: deterministic comment
$dz$ dollar body 18956 ; semicolon inside $dz$
# hash comment 18957
SELECT [bracket_18958] FROM [dbo].[tbl_38];
UPDATE bench_t_15 SET payload = 18959 WHERE id = 15;
DELETE FROM bench_t_16 WHERE id = 0;
-- line 18961: deterministic comment
SELECT 18962 AS id, 'row_18962' AS label;
SELECT nested FROM t WHERE id IN (18963, 18964, 18965);
SELECT * FROM "quoted_18964" WHERE col = E'esc\'18964';
DELETE FROM bench_t_21 WHERE id = 5;
INSERT INTO bench_t_22 (id, payload) VALUES (18966, 'v18966');
SELECT `mysql_18967` FROM `tbl_17`;
SELECT [bracket_18968] FROM [dbo].[tbl_8];
SELECT `mysql_18969` FROM `tbl_19`;
BEGIN; SELECT 18970; COMMIT;
$dz$ dollar body 18971 ; semicolon inside $dz$
/* block header 18972 */
-- line 18973: deterministic comment
INSERT INTO bench_t_30 (id, payload) VALUES (18974, 'v18974');
-- line 18975: deterministic comment
SELECT [bracket_18976] FROM [dbo].[tbl_16];
DELETE FROM bench_t_1 WHERE id = 1;
INSERT INTO bench_t_34 (id, payload) VALUES (18978, 'v18978');
SELECT nested FROM t WHERE id IN (18979, 18980, 18981);
INSERT INTO bench_t_36 (id, payload) VALUES (18980, 'v18980');
BEGIN; SELECT 18981; COMMIT;
-- line 18982: deterministic comment
-- line 18983: deterministic comment
INSERT INTO bench_t_40 (id, payload) VALUES (18984, 'v18984');
DELETE FROM bench_t_9 WHERE id = 9;
$dz$ dollar body 18986 ; semicolon inside $dz$
$dz$ dollar body 18987 ; semicolon inside $dz$
# hash comment 18988
SELECT 18989 AS id, 'row_18989' AS label;
# hash comment 18990
-- line 18991: deterministic comment
INSERT INTO bench_t_48 (id, payload) VALUES (18992, 'v18992');
-- line 18993: deterministic comment
DELETE FROM bench_t_18 WHERE id = 2;
SELECT 18995 AS id, 'row_18995' AS label;
INSERT INTO bench_t_52 (id, payload) VALUES (18996, 'v18996');
BEGIN; SELECT 18997; COMMIT;
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
# hash comment 18999
/*
 * section 76
 * checksum 397a
 */
# hash comment 19000
SELECT `mysql_19005` FROM `tbl_5`;
INSERT INTO bench_t_62 (id, payload) VALUES (19006, 'v19006');
INSERT INTO bench_t_63 (id, payload) VALUES (19007, 'v19007');
SELECT [bracket_19008] FROM [dbo].[tbl_8];
DELETE FROM bench_t_1 WHERE id = 1;
BEGIN; SELECT 19010; COMMIT;
UPDATE bench_t_3 SET payload = 19011 WHERE id = 3;
-- line 19012: deterministic comment
SELECT nested FROM t WHERE id IN (19013, 19014, 19015);
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
$dz$ dollar body 19015 ; semicolon inside $dz$
UPDATE bench_t_8 SET payload = 19016 WHERE id = 8;
SELECT * FROM "quoted_19017" WHERE col = E'esc\'19017';
SELECT nested FROM t WHERE id IN (19018, 19019, 19020);
$dz$ dollar body 19019 ; semicolon inside $dz$
$dz$ dollar body 19020 ; semicolon inside $dz$
/* block header 19021 */
$dz$ dollar body 19022 ; semicolon inside $dz$
WITH cte_19023 AS (SELECT 19023 AS n) SELECT n FROM cte_19023;
UPDATE bench_t_16 SET payload = 19024 WHERE id = 16;
SELECT `mysql_19025` FROM `tbl_25`;
SELECT * FROM "quoted_19026" WHERE col = E'esc\'19026';
DELETE FROM bench_t_19 WHERE id = 3;
SELECT nested FROM t WHERE id IN (19028, 19029, 19030);
WITH cte_19029 AS (SELECT 19029 AS n) SELECT n FROM cte_19029;
DELETE FROM bench_t_22 WHERE id = 6;
BEGIN; SELECT 19031; COMMIT;
-- line 19032: deterministic comment
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
/* block header 19034 */
-- line 19035: deterministic comment
BEGIN; SELECT 19036; COMMIT;
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT [bracket_19038] FROM [dbo].[tbl_38];
SELECT * FROM "quoted_19039" WHERE col = E'esc\'19039';
SELECT nested FROM t WHERE id IN (19040, 19041, 19042);
SELECT [bracket_19041] FROM [dbo].[tbl_1];
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT * FROM "quoted_19043" WHERE col = E'esc\'19043';
BEGIN; SELECT 19044; COMMIT;
INSERT INTO bench_t_101 (id, payload) VALUES (19045, 'v19045');
DELETE FROM bench_t_6 WHERE id = 6;
/* block header 19047 */
INSERT INTO bench_t_104 (id, payload) VALUES (19048, 'v19048');
BEGIN; SELECT 19049; COMMIT;
SELECT nested FROM t WHERE id IN (19050, 19051, 19052);
WITH cte_19051 AS (SELECT 19051 AS n) SELECT n FROM cte_19051;
WITH cte_19052 AS (SELECT 19052 AS n) SELECT n FROM cte_19052;
SELECT nested FROM t WHERE id IN (19053, 19054, 19055);
-- line 19054: deterministic comment
SELECT nested FROM t WHERE id IN (19055, 19056, 19057);
SELECT `mysql_19056` FROM `tbl_6`;
SELECT `mysql_19057` FROM `tbl_7`;
DELETE FROM bench_t_18 WHERE id = 2;
$dz$ dollar body 19059 ; semicolon inside $dz$
-- line 19060: deterministic comment
BEGIN; SELECT 19061; COMMIT;
SELECT [bracket_19062] FROM [dbo].[tbl_22];
UPDATE bench_t_55 SET payload = 19063 WHERE id = 23;
-- line 19064: deterministic comment
WITH cte_19065 AS (SELECT 19065 AS n) SELECT n FROM cte_19065;
INSERT INTO bench_t_122 (id, payload) VALUES (19066, 'v19066');
SELECT nested FROM t WHERE id IN (19067, 19068, 19069);
SELECT nested FROM t WHERE id IN (19068, 19069, 19070);
INSERT INTO bench_t_125 (id, payload) VALUES (19069, 'v19069');
SELECT `mysql_19070` FROM `tbl_20`;
SELECT 19071 AS id, 'row_19071' AS label;
SELECT nested FROM t WHERE id IN (19072, 19073, 19074);
UPDATE bench_t_1 SET payload = 19073 WHERE id = 1;
UPDATE bench_t_2 SET payload = 19074 WHERE id = 2;
-- line 19075: deterministic comment
INSERT INTO bench_t_4 (id, payload) VALUES (19076, 'v19076');
# hash comment 19077
$dz$ dollar body 19078 ; semicolon inside $dz$
DELETE FROM bench_t_7 WHERE id = 7;
# hash comment 19080
SELECT [bracket_19081] FROM [dbo].[tbl_1];
SELECT nested FROM t WHERE id IN (19082, 19083, 19084);
WITH cte_19083 AS (SELECT 19083 AS n) SELECT n FROM cte_19083;
# hash comment 19084
DELETE FROM bench_t_13 WHERE id = 13;
SELECT nested FROM t WHERE id IN (19086, 19087, 19088);
SELECT nested FROM t WHERE id IN (19087, 19088, 19089);
SELECT [bracket_19088] FROM [dbo].[tbl_8];
-- line 19089: deterministic comment
SELECT nested FROM t WHERE id IN (19090, 19091, 19092);
# hash comment 19091
BEGIN; SELECT 19092; COMMIT;
/* block header 19093 */
SELECT 19094 AS id, 'row_19094' AS label;
SELECT 19095 AS id, 'row_19095' AS label;
SELECT * FROM "quoted_19096" WHERE col = E'esc\'19096';
UPDATE bench_t_25 SET payload = 19097 WHERE id = 25;
SELECT [bracket_19098] FROM [dbo].[tbl_18];
SELECT [bracket_19099] FROM [dbo].[tbl_19];
DELETE FROM bench_t_28 WHERE id = 12;
SELECT nested FROM t WHERE id IN (19101, 19102, 19103);
BEGIN; SELECT 19102; COMMIT;
DELETE FROM bench_t_31 WHERE id = 15;
SELECT nested FROM t WHERE id IN (19104, 19105, 19106);
WITH cte_19105 AS (SELECT 19105 AS n) SELECT n FROM cte_19105;
SELECT `mysql_19106` FROM `tbl_6`;
DELETE FROM bench_t_3 WHERE id = 3;
/* block header 19108 */
SELECT [bracket_19109] FROM [dbo].[tbl_29];
/* block header 19110 */
-- line 19111: deterministic comment
WITH cte_19112 AS (SELECT 19112 AS n) SELECT n FROM cte_19112;
SELECT * FROM "quoted_19113" WHERE col = E'esc\'19113';
WITH cte_19114 AS (SELECT 19114 AS n) SELECT n FROM cte_19114;
DELETE FROM bench_t_11 WHERE id = 11;
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
# hash comment 19117
$dz$ dollar body 19118 ; semicolon inside $dz$
BEGIN; SELECT 19119; COMMIT;
SELECT 19120 AS id, 'row_19120' AS label;
BEGIN; SELECT 19121; COMMIT;
WITH cte_19122 AS (SELECT 19122 AS n) SELECT n FROM cte_19122;
BEGIN; SELECT 19123; COMMIT;
SELECT 19124 AS id, 'row_19124' AS label;
SELECT [bracket_19125] FROM [dbo].[tbl_5];
SELECT 19126 AS id, 'row_19126' AS label;
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT 19128 AS id, 'row_19128' AS label;
BEGIN; SELECT 19129; COMMIT;
WITH cte_19130 AS (SELECT 19130 AS n) SELECT n FROM cte_19130;
SELECT nested FROM t WHERE id IN (19131, 19132, 19133);
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
$dz$ dollar body 19133 ; semicolon inside $dz$
BEGIN; SELECT 19134; COMMIT;
SELECT 19135 AS id, 'row_19135' AS label;
INSERT INTO bench_t_64 (id, payload) VALUES (19136, 'v19136');
SELECT 19137 AS id, 'row_19137' AS label;
-- line 19138: deterministic comment
WITH cte_19139 AS (SELECT 19139 AS n) SELECT n FROM cte_19139;
SELECT nested FROM t WHERE id IN (19140, 19141, 19142);
SELECT `mysql_19141` FROM `tbl_41`;
/* block header 19142 */
SELECT `mysql_19143` FROM `tbl_43`;
SELECT `mysql_19144` FROM `tbl_44`;
SELECT * FROM "quoted_19145" WHERE col = E'esc\'19145';
SELECT nested FROM t WHERE id IN (19146, 19147, 19148);
-- line 19147: deterministic comment
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
/* block header 19149 */
SELECT [bracket_19150] FROM [dbo].[tbl_30];
/* block header 19151 */
SELECT * FROM "quoted_19152" WHERE col = E'esc\'19152';
SELECT * FROM "quoted_19153" WHERE col = E'esc\'19153';
SELECT [bracket_19154] FROM [dbo].[tbl_34];
-- line 19155: deterministic comment
BEGIN; SELECT 19156; COMMIT;
SELECT `mysql_19157` FROM `tbl_7`;
SELECT * FROM "quoted_19158" WHERE col = E'esc\'19158';
BEGIN; SELECT 19159; COMMIT;
/* block header 19160 */
SELECT `mysql_19161` FROM `tbl_11`;
BEGIN; SELECT 19162; COMMIT;
INSERT INTO bench_t_91 (id, payload) VALUES (19163, 'v19163');
# hash comment 19164
-- line 19165: deterministic comment
SELECT nested FROM t WHERE id IN (19166, 19167, 19168);
SELECT 19167 AS id, 'row_19167' AS label;
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT [bracket_19169] FROM [dbo].[tbl_9];
$dz$ dollar body 19170 ; semicolon inside $dz$
INSERT INTO bench_t_99 (id, payload) VALUES (19171, 'v19171');
WITH cte_19172 AS (SELECT 19172 AS n) SELECT n FROM cte_19172;
SELECT nested FROM t WHERE id IN (19173, 19174, 19175);
DELETE FROM bench_t_6 WHERE id = 6;
/* block header 19175 */
INSERT INTO bench_t_104 (id, payload) VALUES (19176, 'v19176');
# hash comment 19177
INSERT INTO bench_t_106 (id, payload) VALUES (19178, 'v19178');
$dz$ dollar body 19179 ; semicolon inside $dz$
WITH cte_19180 AS (SELECT 19180 AS n) SELECT n FROM cte_19180;
BEGIN; SELECT 19181; COMMIT;
INSERT INTO bench_t_110 (id, payload) VALUES (19182, 'v19182');
SELECT `mysql_19183` FROM `tbl_33`;
$dz$ dollar body 19184 ; semicolon inside $dz$
/* block header 19185 */
BEGIN; SELECT 19186; COMMIT;
UPDATE bench_t_51 SET payload = 19187 WHERE id = 19;
-- line 19188: deterministic comment
SELECT [bracket_19189] FROM [dbo].[tbl_29];
DELETE FROM bench_t_22 WHERE id = 6;
SELECT `mysql_19191` FROM `tbl_41`;
SELECT 19192 AS id, 'row_19192' AS label;
-- line 19193: deterministic comment
SELECT [bracket_19194] FROM [dbo].[tbl_34];
SELECT [bracket_19195] FROM [dbo].[tbl_35];
SELECT nested FROM t WHERE id IN (19196, 19197, 19198);
UPDATE bench_t_61 SET payload = 19197 WHERE id = 29;
-- line 19198: deterministic comment
-- line 19199: deterministic comment
SELECT * FROM "quoted_19200" WHERE col = E'esc\'19200';
SELECT * FROM "quoted_19201" WHERE col = E'esc\'19201';
WITH cte_19202 AS (SELECT 19202 AS n) SELECT n FROM cte_19202;
SELECT `mysql_19203` FROM `tbl_3`;
# hash comment 19204
INSERT INTO bench_t_5 (id, payload) VALUES (19205, 'v19205');
DELETE FROM bench_t_6 WHERE id = 6;
-- line 19207: deterministic comment
/* block header 19208 */
UPDATE bench_t_9 SET payload = 19209 WHERE id = 9;
SELECT nested FROM t WHERE id IN (19210, 19211, 19212);
$dz$ dollar body 19211 ; semicolon inside $dz$
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
WITH cte_19213 AS (SELECT 19213 AS n) SELECT n FROM cte_19213;
SELECT * FROM "quoted_19214" WHERE col = E'esc\'19214';
WITH cte_19215 AS (SELECT 19215 AS n) SELECT n FROM cte_19215;
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
/* block header 19217 */
$dz$ dollar body 19218 ; semicolon inside $dz$
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
$dz$ dollar body 19221 ; semicolon inside $dz$
SELECT 19222 AS id, 'row_19222' AS label;
# hash comment 19223
SELECT * FROM "quoted_19224" WHERE col = E'esc\'19224';
INSERT INTO bench_t_25 (id, payload) VALUES (19225, 'v19225');
WITH cte_19226 AS (SELECT 19226 AS n) SELECT n FROM cte_19226;
DELETE FROM bench_t_27 WHERE id = 11;
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
INSERT INTO bench_t_29 (id, payload) VALUES (19229, 'v19229');
SELECT nested FROM t WHERE id IN (19230, 19231, 19232);
INSERT INTO bench_t_31 (id, payload) VALUES (19231, 'v19231');
$dz$ dollar body 19232 ; semicolon inside $dz$
$dz$ dollar body 19233 ; semicolon inside $dz$
SELECT nested FROM t WHERE id IN (19234, 19235, 19236);
SELECT nested FROM t WHERE id IN (19235, 19236, 19237);
SELECT [bracket_19236] FROM [dbo].[tbl_36];
/* block header 19237 */
/* block header 19238 */
INSERT INTO bench_t_39 (id, payload) VALUES (19239, 'O''Brien');
SELECT nested FROM t WHERE id IN (19240, 19241, 19242);
SELECT nested FROM t WHERE id IN (19241, 19242, 19243);
SELECT * FROM "quoted_19242" WHERE col = E'esc\'19242';
/* block header 19243 */
BEGIN; SELECT 19244; COMMIT;
INSERT INTO bench_t_45 (id, payload) VALUES (19245, 'v19245');
SELECT nested FROM t WHERE id IN (19246, 19247, 19248);
DELETE FROM bench_t_15 WHERE id = 15;
DELETE FROM bench_t_16 WHERE id = 0;
INSERT INTO bench_t_49 (id, payload) VALUES (19249, 'v19249');
/*
 * section 77
 * checksum 8af9
 */
SELECT [bracket_19250] FROM [dbo].[tbl_10];
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
WITH cte_19256 AS (SELECT 19256 AS n) SELECT n FROM cte_19256;
/* block header 19257 */
SELECT * FROM "quoted_19258" WHERE col = E'esc\'19258';
/* block header 19259 */
UPDATE bench_t_60 SET payload = 19260 WHERE id = 28;
DELETE FROM bench_t_29 WHERE id = 13;
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
$dz$ dollar body 19264 ; semicolon inside $dz$
INSERT INTO bench_t_65 (id, payload) VALUES (19265, 'v19265');
SELECT 19266 AS id, 'row_19266' AS label;
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
UPDATE bench_t_4 SET payload = 19268 WHERE id = 4;
INSERT INTO bench_t_69 (id, payload) VALUES (19269, 'v19269');
-- line 19270: deterministic comment
/* block header 19271 */
SELECT `mysql_19272` FROM `tbl_22`;
SELECT [bracket_19273] FROM [dbo].[tbl_33];
DELETE FROM bench_t_10 WHERE id = 10;
/* block header 19275 */
DELETE FROM bench_t_12 WHERE id = 12;
-- line 19277: deterministic comment
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT [bracket_19279] FROM [dbo].[tbl_39];
DELETE FROM bench_t_16 WHERE id = 0;
WITH cte_19281 AS (SELECT 19281 AS n) SELECT n FROM cte_19281;
BEGIN; SELECT 19282; COMMIT;
BEGIN; SELECT 19283; COMMIT;
SELECT 19284 AS id, 'row_19284' AS label;
SELECT * FROM "quoted_19285" WHERE col = E'esc\'19285';
# hash comment 19286
SELECT `mysql_19287` FROM `tbl_37`;
BEGIN; SELECT 19288; COMMIT;
-- line 19289: deterministic comment
UPDATE bench_t_26 SET payload = 19290 WHERE id = 26;
SELECT `mysql_19291` FROM `tbl_41`;
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT [bracket_19293] FROM [dbo].[tbl_13];
SELECT `mysql_19294` FROM `tbl_44`;
# hash comment 19295
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT [bracket_19298] FROM [dbo].[tbl_18];
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
DELETE FROM bench_t_4 WHERE id = 4;
UPDATE bench_t_37 SET payload = 19301 WHERE id = 5;
SELECT [bracket_19302] FROM [dbo].[tbl_22];
SELECT nested FROM t WHERE id IN (19303, 19304, 19305);
SELECT * FROM "quoted_19304" WHERE col = E'esc\'19304';
-- line 19305: deterministic comment
/* block header 19306 */
-- line 19307: deterministic comment
$dz$ dollar body 19308 ; semicolon inside $dz$
WITH cte_19309 AS (SELECT 19309 AS n) SELECT n FROM cte_19309;
WITH cte_19310 AS (SELECT 19310 AS n) SELECT n FROM cte_19310;
DELETE FROM bench_t_15 WHERE id = 15;
SELECT [bracket_19312] FROM [dbo].[tbl_32];
SELECT nested FROM t WHERE id IN (19313, 19314, 19315);
$dz$ dollar body 19314 ; semicolon inside $dz$
$dz$ dollar body 19315 ; semicolon inside $dz$
SELECT `mysql_19316` FROM `tbl_16`;
-- line 19317: deterministic comment
/* block header 19318 */
$dz$ dollar body 19319 ; semicolon inside $dz$
BEGIN; SELECT 19320; COMMIT;
SELECT [bracket_19321] FROM [dbo].[tbl_1];
WITH cte_19322 AS (SELECT 19322 AS n) SELECT n FROM cte_19322;
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
$dz$ dollar body 19324 ; semicolon inside $dz$
-- line 19325: deterministic comment
WITH cte_19326 AS (SELECT 19326 AS n) SELECT n FROM cte_19326;
UPDATE bench_t_63 SET payload = 19327 WHERE id = 31;
$dz$ dollar body 19328 ; semicolon inside $dz$
SELECT * FROM "quoted_19329" WHERE col = E'esc\'19329';
$dz$ dollar body 19330 ; semicolon inside $dz$
SELECT [bracket_19331] FROM [dbo].[tbl_11];
SELECT nested FROM t WHERE id IN (19332, 19333, 19334);
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
INSERT INTO bench_t_6 (id, payload) VALUES (19334, 'v19334');
/* block header 19335 */
/* block header 19336 */
SELECT `mysql_19337` FROM `tbl_37`;
SELECT nested FROM t WHERE id IN (19338, 19339, 19340);
SELECT nested FROM t WHERE id IN (19339, 19340, 19341);
UPDATE bench_t_12 SET payload = 19340 WHERE id = 12;
SELECT [bracket_19341] FROM [dbo].[tbl_21];
# hash comment 19342
SELECT nested FROM t WHERE id IN (19343, 19344, 19345);
BEGIN; SELECT 19344; COMMIT;
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT [bracket_19346] FROM [dbo].[tbl_26];
SELECT * FROM "quoted_19347" WHERE col = E'esc\'19347';
SELECT 19348 AS id, 'row_19348' AS label;
WITH cte_19349 AS (SELECT 19349 AS n) SELECT n FROM cte_19349;
DELETE FROM bench_t_22 WHERE id = 6;
SELECT [bracket_19351] FROM [dbo].[tbl_31];
/* block header 19352 */
DELETE FROM bench_t_25 WHERE id = 9;
UPDATE bench_t_26 SET payload = 19354 WHERE id = 26;
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT * FROM "quoted_19356" WHERE col = E'esc\'19356';
SELECT * FROM "quoted_19357" WHERE col = E'esc\'19357';
DELETE FROM bench_t_30 WHERE id = 14;
-- line 19359: deterministic comment
SELECT 19360 AS id, 'row_19360' AS label;
UPDATE bench_t_33 SET payload = 19361 WHERE id = 1;
BEGIN; SELECT 19362; COMMIT;
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
WITH cte_19364 AS (SELECT 19364 AS n) SELECT n FROM cte_19364;
BEGIN; SELECT 19365; COMMIT;
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
INSERT INTO bench_t_39 (id, payload) VALUES (19367, 'v19367');
/* block header 19368 */
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT 19370 AS id, 'row_19370' AS label;
UPDATE bench_t_43 SET payload = 19371 WHERE id = 11;
UPDATE bench_t_44 SET payload = 19372 WHERE id = 12;
SELECT * FROM "quoted_19373" WHERE col = E'esc\'19373';
SELECT * FROM "quoted_19374" WHERE col = E'esc\'19374';
SELECT nested FROM t WHERE id IN (19375, 19376, 19377);
DELETE FROM bench_t_16 WHERE id = 0;
WITH cte_19377 AS (SELECT 19377 AS n) SELECT n FROM cte_19377;
$dz$ dollar body 19378 ; semicolon inside $dz$
-- line 19379: deterministic comment
$dz$ dollar body 19380 ; semicolon inside $dz$
SELECT 19381 AS id, 'row_19381' AS label;
-- line 19382: deterministic comment
-- line 19383: deterministic comment
SELECT `mysql_19384` FROM `tbl_34`;
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT nested FROM t WHERE id IN (19386, 19387, 19388);
SELECT `mysql_19387` FROM `tbl_37`;
-- line 19388: deterministic comment
SELECT * FROM "quoted_19389" WHERE col = E'esc\'19389';
# hash comment 19390
SELECT * FROM "quoted_19391" WHERE col = E'esc\'19391';
SELECT * FROM "quoted_19392" WHERE col = E'esc\'19392';
$dz$ dollar body 19393 ; semicolon inside $dz$
INSERT INTO bench_t_66 (id, payload) VALUES (19394, 'v19394');
$dz$ dollar body 19395 ; semicolon inside $dz$
$dz$ dollar body 19396 ; semicolon inside $dz$
SELECT [bracket_19397] FROM [dbo].[tbl_37];
SELECT 19398 AS id, 'row_19398' AS label;
SELECT `mysql_19399` FROM `tbl_49`;
/* block header 19400 */
-- line 19401: deterministic comment
SELECT [bracket_19402] FROM [dbo].[tbl_2];
SELECT 19403 AS id, 'row_19403' AS label;
-- line 19404: deterministic comment
/* block header 19405 */
SELECT [bracket_19406] FROM [dbo].[tbl_6];
SELECT nested FROM t WHERE id IN (19407, 19408, 19409);
UPDATE bench_t_16 SET payload = 19408 WHERE id = 16;
UPDATE bench_t_17 SET payload = 19409 WHERE id = 17;
SELECT [bracket_19410] FROM [dbo].[tbl_10];
BEGIN; SELECT 19411; COMMIT;
SELECT [bracket_19412] FROM [dbo].[tbl_12];
UPDATE bench_t_21 SET payload = 19413 WHERE id = 21;
# hash comment 19414
$dz$ dollar body 19415 ; semicolon inside $dz$
BEGIN; SELECT 19416; COMMIT;
$dz$ dollar body 19417 ; semicolon inside $dz$
# hash comment 19418
/* block header 19419 */
SELECT nested FROM t WHERE id IN (19420, 19421, 19422);
SELECT [bracket_19421] FROM [dbo].[tbl_21];
/* block header 19422 */
SELECT * FROM "quoted_19423" WHERE col = E'esc\'19423';
SELECT nested FROM t WHERE id IN (19424, 19425, 19426);
# hash comment 19425
# hash comment 19426
SELECT * FROM "quoted_19427" WHERE col = E'esc\'19427';
SELECT [bracket_19428] FROM [dbo].[tbl_28];
SELECT `mysql_19429` FROM `tbl_29`;
SELECT 19430 AS id, 'row_19430' AS label;
INSERT INTO bench_t_103 (id, payload) VALUES (19431, 'v19431');
-- line 19432: deterministic comment
UPDATE bench_t_41 SET payload = 19433 WHERE id = 9;
SELECT `mysql_19434` FROM `tbl_34`;
SELECT * FROM "quoted_19435" WHERE col = E'esc\'19435';
UPDATE bench_t_44 SET payload = 19436 WHERE id = 12;
SELECT * FROM "quoted_19437" WHERE col = E'esc\'19437';
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT 19440 AS id, 'row_19440' AS label;
/* block header 19441 */
BEGIN; SELECT 19442; COMMIT;
SELECT `mysql_19443` FROM `tbl_43`;
SELECT [bracket_19444] FROM [dbo].[tbl_4];
# hash comment 19445
SELECT `mysql_19446` FROM `tbl_46`;
SELECT nested FROM t WHERE id IN (19447, 19448, 19449);
# hash comment 19448
SELECT * FROM "quoted_19449" WHERE col = E'esc\'19449';
WITH cte_19450 AS (SELECT 19450 AS n) SELECT n FROM cte_19450;
UPDATE bench_t_59 SET payload = 19451 WHERE id = 27;
# hash comment 19452
SELECT 19453 AS id, 'row_19453' AS label;
# hash comment 19454
SELECT `mysql_19455` FROM `tbl_5`;
DELETE FROM bench_t_0 WHERE id = 0;
UPDATE bench_t_1 SET payload = 19457 WHERE id = 1;
SELECT nested FROM t WHERE id IN (19458, 19459, 19460);
SELECT nested FROM t WHERE id IN (19459, 19460, 19461);
INSERT INTO bench_t_4 (id, payload) VALUES (19460, 'v19460');
INSERT INTO bench_t_5 (id, payload) VALUES (19461, 'v19461');
SELECT nested FROM t WHERE id IN (19462, 19463, 19464);
SELECT nested FROM t WHERE id IN (19463, 19464, 19465);
SELECT nested FROM t WHERE id IN (19464, 19465, 19466);
/* block header 19465 */
UPDATE bench_t_10 SET payload = 19466 WHERE id = 10;
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT [bracket_19468] FROM [dbo].[tbl_28];
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT 19470 AS id, 'row_19470' AS label;
BEGIN; SELECT 19471; COMMIT;
$dz$ dollar body 19472 ; semicolon inside $dz$
SELECT 19473 AS id, 'row_19473' AS label;
SELECT 19474 AS id, 'row_19474' AS label;
SELECT [bracket_19475] FROM [dbo].[tbl_35];
BEGIN; SELECT 19476; COMMIT;
SELECT `mysql_19477` FROM `tbl_27`;
# hash comment 19478
SELECT `mysql_19479` FROM `tbl_29`;
SELECT [bracket_19480] FROM [dbo].[tbl_0];
SELECT * FROM "quoted_19481" WHERE col = E'esc\'19481';
UPDATE bench_t_26 SET payload = 19482 WHERE id = 26;
$dz$ dollar body 19483 ; semicolon inside $dz$
SELECT 19484 AS id, 'row_19484' AS label;
UPDATE bench_t_29 SET payload = 19485 WHERE id = 29;
-- line 19486: deterministic comment
SELECT [bracket_19487] FROM [dbo].[tbl_7];
UPDATE bench_t_32 SET payload = 19488 WHERE id = 0;
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT [bracket_19490] FROM [dbo].[tbl_10];
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT * FROM "quoted_19492" WHERE col = E'esc\'19492';
SELECT [bracket_19493] FROM [dbo].[tbl_13];
$dz$ dollar body 19494 ; semicolon inside $dz$
SELECT nested FROM t WHERE id IN (19495, 19496, 19497);
BEGIN; SELECT 19496; COMMIT;
SELECT nested FROM t WHERE id IN (19497, 19498, 19499);
SELECT nested FROM t WHERE id IN (19498, 19499, 19500);
$dz$ dollar body 19499 ; semicolon inside $dz$
/*
 * section 78
 * checksum 34b7
 */
SELECT * FROM "quoted_19500" WHERE col = E'esc\'19500';
INSERT INTO bench_t_49 (id, payload) VALUES (19505, 'v19505');
SELECT * FROM "quoted_19506" WHERE col = E'esc\'19506';
$dz$ dollar body 19507 ; semicolon inside $dz$
INSERT INTO bench_t_52 (id, payload) VALUES (19508, 'v19508');
UPDATE bench_t_53 SET payload = 19509 WHERE id = 21;
-- line 19510: deterministic comment
DELETE FROM bench_t_23 WHERE id = 7;
SELECT `mysql_19512` FROM `tbl_12`;
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
/* block header 19514 */
WITH cte_19515 AS (SELECT 19515 AS n) SELECT n FROM cte_19515;
DELETE FROM bench_t_28 WHERE id = 12;
-- line 19517: deterministic comment
UPDATE bench_t_62 SET payload = 19518 WHERE id = 30;
SELECT [bracket_19519] FROM [dbo].[tbl_39];
SELECT * FROM "quoted_19520" WHERE col = E'esc\'19520';
SELECT [bracket_19521] FROM [dbo].[tbl_1];
-- line 19522: deterministic comment
-- line 19523: deterministic comment
BEGIN; SELECT 19524; COMMIT;
WITH cte_19525 AS (SELECT 19525 AS n) SELECT n FROM cte_19525;
SELECT * FROM "quoted_19526" WHERE col = E'esc\'19526';
WITH cte_19527 AS (SELECT 19527 AS n) SELECT n FROM cte_19527;
WITH cte_19528 AS (SELECT 19528 AS n) SELECT n FROM cte_19528;
DELETE FROM bench_t_9 WHERE id = 9;
SELECT [bracket_19530] FROM [dbo].[tbl_10];
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
DELETE FROM bench_t_12 WHERE id = 12;
SELECT * FROM "quoted_19533" WHERE col = E'esc\'19533';
SELECT 19534 AS id, 'row_19534' AS label;
UPDATE bench_t_15 SET payload = 19535 WHERE id = 15;
SELECT `mysql_19536` FROM `tbl_36`;
SELECT `mysql_19537` FROM `tbl_37`;
WITH cte_19538 AS (SELECT 19538 AS n) SELECT n FROM cte_19538;
SELECT * FROM "quoted_19539" WHERE col = E'esc\'19539';
SELECT `mysql_19540` FROM `tbl_40`;
$dz$ dollar body 19541 ; semicolon inside $dz$
SELECT * FROM "quoted_19542" WHERE col = E'esc\'19542';
-- line 19543: deterministic comment
SELECT [bracket_19544] FROM [dbo].[tbl_24];
UPDATE bench_t_25 SET payload = 19545 WHERE id = 25;
SELECT * FROM "quoted_19546" WHERE col = E'esc\'19546';
/* block header 19547 */
WITH cte_19548 AS (SELECT 19548 AS n) SELECT n FROM cte_19548;
DELETE FROM bench_t_29 WHERE id = 13;
SELECT [bracket_19550] FROM [dbo].[tbl_30];
UPDATE bench_t_31 SET payload = 19551 WHERE id = 31;
SELECT `mysql_19552` FROM `tbl_2`;
$dz$ dollar body 19553 ; semicolon inside $dz$
BEGIN; SELECT 19554; COMMIT;
SELECT `mysql_19555` FROM `tbl_5`;
# hash comment 19556
BEGIN; SELECT 19557; COMMIT;
/* block header 19558 */
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
/* block header 19560 */
SELECT `mysql_19561` FROM `tbl_11`;
DELETE FROM bench_t_10 WHERE id = 10;
BEGIN; SELECT 19563; COMMIT;
SELECT 19564 AS id, 'row_19564' AS label;
DELETE FROM bench_t_13 WHERE id = 13;
INSERT INTO bench_t_110 (id, payload) VALUES (19566, 'v19566');
WITH cte_19567 AS (SELECT 19567 AS n) SELECT n FROM cte_19567;
SELECT * FROM "quoted_19568" WHERE col = E'esc\'19568';
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
WITH cte_19570 AS (SELECT 19570 AS n) SELECT n FROM cte_19570;
-- line 19571: deterministic comment
/* block header 19572 */
# hash comment 19573
/* block header 19574 */
SELECT * FROM "quoted_19575" WHERE col = E'esc\'19575';
WITH cte_19576 AS (SELECT 19576 AS n) SELECT n FROM cte_19576;
SELECT * FROM "quoted_19577" WHERE col = E'esc\'19577';
-- line 19578: deterministic comment
-- line 19579: deterministic comment
SELECT [bracket_19580] FROM [dbo].[tbl_20];
/* block header 19581 */
BEGIN; SELECT 19582; COMMIT;
BEGIN; SELECT 19583; COMMIT;
DELETE FROM bench_t_0 WHERE id = 0;
INSERT INTO bench_t_1 (id, payload) VALUES (19585, 'v19585');
SELECT nested FROM t WHERE id IN (19586, 19587, 19588);
UPDATE bench_t_3 SET payload = 19587 WHERE id = 3;
SELECT * FROM "quoted_19588" WHERE col = E'esc\'19588';
BEGIN; SELECT 19589; COMMIT;
# hash comment 19590
/* block header 19591 */
-- line 19592: deterministic comment
UPDATE bench_t_9 SET payload = 19593 WHERE id = 9;
WITH cte_19594 AS (SELECT 19594 AS n) SELECT n FROM cte_19594;
SELECT [bracket_19595] FROM [dbo].[tbl_35];
INSERT INTO bench_t_12 (id, payload) VALUES (19596, 'v19596');
SELECT `mysql_19597` FROM `tbl_47`;
SELECT [bracket_19598] FROM [dbo].[tbl_38];
# hash comment 19599
WITH cte_19600 AS (SELECT 19600 AS n) SELECT n FROM cte_19600;
DELETE FROM bench_t_17 WHERE id = 1;
BEGIN; SELECT 19602; COMMIT;
UPDATE bench_t_19 SET payload = 19603 WHERE id = 19;
WITH cte_19604 AS (SELECT 19604 AS n) SELECT n FROM cte_19604;
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
/* block header 19606 */
# hash comment 19607
UPDATE bench_t_24 SET payload = 19608 WHERE id = 24;
UPDATE bench_t_25 SET payload = 19609 WHERE id = 25;
WITH cte_19610 AS (SELECT 19610 AS n) SELECT n FROM cte_19610;
SELECT * FROM "quoted_19611" WHERE col = E'esc\'19611';
$dz$ dollar body 19612 ; semicolon inside $dz$
WITH cte_19613 AS (SELECT 19613 AS n) SELECT n FROM cte_19613;
DELETE FROM bench_t_30 WHERE id = 14;
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
BEGIN; SELECT 19616; COMMIT;
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
WITH cte_19618 AS (SELECT 19618 AS n) SELECT n FROM cte_19618;
SELECT nested FROM t WHERE id IN (19619, 19620, 19621);
BEGIN; SELECT 19620; COMMIT;
-- line 19621: deterministic comment
SELECT 19622 AS id, 'row_19622' AS label;
SELECT * FROM "quoted_19623" WHERE col = E'esc\'19623';
SELECT [bracket_19624] FROM [dbo].[tbl_24];
/* block header 19625 */
SELECT 19626 AS id, 'row_19626' AS label;
$dz$ dollar body 19627 ; semicolon inside $dz$
SELECT [bracket_19628] FROM [dbo].[tbl_28];
BEGIN; SELECT 19629; COMMIT;
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
INSERT INTO bench_t_47 (id, payload) VALUES (19631, 'v19631');
UPDATE bench_t_48 SET payload = 19632 WHERE id = 16;
SELECT * FROM "quoted_19633" WHERE col = E'esc\'19633';
SELECT `mysql_19634` FROM `tbl_34`;
$dz$ dollar body 19635 ; semicolon inside $dz$
SELECT [bracket_19636] FROM [dbo].[tbl_36];
SELECT nested FROM t WHERE id IN (19637, 19638, 19639);
SELECT * FROM "quoted_19638" WHERE col = E'esc\'19638';
SELECT `mysql_19639` FROM `tbl_39`;
/* block header 19640 */
/* block header 19641 */
/* block header 19642 */
-- line 19643: deterministic comment
DELETE FROM bench_t_28 WHERE id = 12;
SELECT [bracket_19645] FROM [dbo].[tbl_5];
# hash comment 19646
/* block header 19647 */
BEGIN; SELECT 19648; COMMIT;
INSERT INTO bench_t_65 (id, payload) VALUES (19649, 'v19649');
-- line 19650: deterministic comment
DELETE FROM bench_t_3 WHERE id = 3;
# hash comment 19652
DELETE FROM bench_t_5 WHERE id = 5;
$dz$ dollar body 19654 ; semicolon inside $dz$
-- line 19655: deterministic comment
SELECT `mysql_19656` FROM `tbl_6`;
WITH cte_19657 AS (SELECT 19657 AS n) SELECT n FROM cte_19657;
BEGIN; SELECT 19658; COMMIT;
SELECT * FROM "quoted_19659" WHERE col = E'esc\'19659';
SELECT [bracket_19660] FROM [dbo].[tbl_20];
UPDATE bench_t_13 SET payload = 19661 WHERE id = 13;
SELECT nested FROM t WHERE id IN (19662, 19663, 19664);
-- line 19663: deterministic comment
-- line 19664: deterministic comment
SELECT * FROM "quoted_19665" WHERE col = E'esc\'19665';
# hash comment 19666
INSERT INTO bench_t_83 (id, payload) VALUES (19667, 'v19667');
SELECT `mysql_19668` FROM `tbl_18`;
SELECT [bracket_19669] FROM [dbo].[tbl_29];
SELECT * FROM "quoted_19670" WHERE col = E'esc\'19670';
SELECT [bracket_19671] FROM [dbo].[tbl_31];
SELECT `mysql_19672` FROM `tbl_22`;
UPDATE bench_t_25 SET payload = 19673 WHERE id = 25;
-- line 19674: deterministic comment
UPDATE bench_t_27 SET payload = 19675 WHERE id = 27;
WITH cte_19676 AS (SELECT 19676 AS n) SELECT n FROM cte_19676;
SELECT [bracket_19677] FROM [dbo].[tbl_37];
# hash comment 19678
INSERT INTO bench_t_95 (id, payload) VALUES (19679, 'O''Brien');
BEGIN; SELECT 19680; COMMIT;
SELECT * FROM "quoted_19681" WHERE col = E'esc\'19681';
SELECT nested FROM t WHERE id IN (19682, 19683, 19684);
DELETE FROM bench_t_3 WHERE id = 3;
SELECT * FROM "quoted_19684" WHERE col = E'esc\'19684';
-- line 19685: deterministic comment
SELECT `mysql_19686` FROM `tbl_36`;
SELECT `mysql_19687` FROM `tbl_37`;
SELECT 19688 AS id, 'row_19688' AS label;
DELETE FROM bench_t_9 WHERE id = 9;
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT `mysql_19691` FROM `tbl_41`;
DELETE FROM bench_t_12 WHERE id = 12;
/* block header 19693 */
/* block header 19694 */
SELECT [bracket_19695] FROM [dbo].[tbl_15];
SELECT nested FROM t WHERE id IN (19696, 19697, 19698);
SELECT [bracket_19697] FROM [dbo].[tbl_17];
-- line 19698: deterministic comment
SELECT nested FROM t WHERE id IN (19699, 19700, 19701);
/* block header 19700 */
BEGIN; SELECT 19701; COMMIT;
WITH cte_19702 AS (SELECT 19702 AS n) SELECT n FROM cte_19702;
UPDATE bench_t_55 SET payload = 19703 WHERE id = 23;
BEGIN; SELECT 19704; COMMIT;
SELECT 19705 AS id, 'row_19705' AS label;
/* block header 19706 */
/* block header 19707 */
UPDATE bench_t_60 SET payload = 19708 WHERE id = 28;
BEGIN; SELECT 19709; COMMIT;
BEGIN; SELECT 19710; COMMIT;
SELECT * FROM "quoted_19711" WHERE col = E'esc\'19711';
SELECT `mysql_19712` FROM `tbl_12`;
DELETE FROM bench_t_1 WHERE id = 1;
# hash comment 19714
SELECT `mysql_19715` FROM `tbl_15`;
SELECT * FROM "quoted_19716" WHERE col = E'esc\'19716';
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
# hash comment 19718
/* block header 19719 */
# hash comment 19720
SELECT nested FROM t WHERE id IN (19721, 19722, 19723);
SELECT 19722 AS id, 'row_19722' AS label;
SELECT `mysql_19723` FROM `tbl_23`;
SELECT [bracket_19724] FROM [dbo].[tbl_4];
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
-- line 19726: deterministic comment
BEGIN; SELECT 19727; COMMIT;
SELECT * FROM "quoted_19728" WHERE col = E'esc\'19728';
$dz$ dollar body 19729 ; semicolon inside $dz$
WITH cte_19730 AS (SELECT 19730 AS n) SELECT n FROM cte_19730;
DELETE FROM bench_t_19 WHERE id = 3;
WITH cte_19732 AS (SELECT 19732 AS n) SELECT n FROM cte_19732;
/* block header 19733 */
UPDATE bench_t_22 SET payload = 19734 WHERE id = 22;
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT * FROM "quoted_19736" WHERE col = E'esc\'19736';
BEGIN; SELECT 19737; COMMIT;
UPDATE bench_t_26 SET payload = 19738 WHERE id = 26;
WITH cte_19739 AS (SELECT 19739 AS n) SELECT n FROM cte_19739;
SELECT `mysql_19740` FROM `tbl_40`;
UPDATE bench_t_29 SET payload = 19741 WHERE id = 29;
-- line 19742: deterministic comment
WITH cte_19743 AS (SELECT 19743 AS n) SELECT n FROM cte_19743;
SELECT nested FROM t WHERE id IN (19744, 19745, 19746);
SELECT nested FROM t WHERE id IN (19745, 19746, 19747);
BEGIN; SELECT 19746; COMMIT;
SELECT nested FROM t WHERE id IN (19747, 19748, 19749);
$dz$ dollar body 19748 ; semicolon inside $dz$
SELECT * FROM "quoted_19749" WHERE col = E'esc\'19749';
/*
 * section 79
 * checksum 5e97
 */
SELECT * FROM "quoted_19750" WHERE col = E'esc\'19750';
$dz$ dollar body 19755 ; semicolon inside $dz$
-- line 19756: deterministic comment
SELECT * FROM "quoted_19757" WHERE col = E'esc\'19757';
SELECT * FROM "quoted_19758" WHERE col = E'esc\'19758';
WITH cte_19759 AS (SELECT 19759 AS n) SELECT n FROM cte_19759;
SELECT [bracket_19760] FROM [dbo].[tbl_0];
SELECT * FROM "quoted_19761" WHERE col = E'esc\'19761';
SELECT `mysql_19762` FROM `tbl_12`;
UPDATE bench_t_51 SET payload = 19763 WHERE id = 19;
SELECT nested FROM t WHERE id IN (19764, 19765, 19766);
# hash comment 19765
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
-- line 19767: deterministic comment
SELECT nested FROM t WHERE id IN (19768, 19769, 19770);
SELECT 19769 AS id, 'row_19769' AS label;
SELECT 19770 AS id, 'row_19770' AS label;
DELETE FROM bench_t_27 WHERE id = 11;
SELECT * FROM "quoted_19772" WHERE col = E'esc\'19772';
INSERT INTO bench_t_61 (id, payload) VALUES (19773, 'v19773');
/* block header 19774 */
/* block header 19775 */
DELETE FROM bench_t_0 WHERE id = 0;
DELETE FROM bench_t_1 WHERE id = 1;
UPDATE bench_t_2 SET payload = 19778 WHERE id = 2;
SELECT `mysql_19779` FROM `tbl_29`;
SELECT * FROM "quoted_19780" WHERE col = E'esc\'19780';
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT * FROM "quoted_19782" WHERE col = E'esc\'19782';
BEGIN; SELECT 19783; COMMIT;
DELETE FROM bench_t_8 WHERE id = 8;
WITH cte_19785 AS (SELECT 19785 AS n) SELECT n FROM cte_19785;
DELETE FROM bench_t_10 WHERE id = 10;
# hash comment 19787
$dz$ dollar body 19788 ; semicolon inside $dz$
UPDATE bench_t_13 SET payload = 19789 WHERE id = 13;
SELECT * FROM "quoted_19790" WHERE col = E'esc\'19790';
$dz$ dollar body 19791 ; semicolon inside $dz$
UPDATE bench_t_16 SET payload = 19792 WHERE id = 16;
INSERT INTO bench_t_81 (id, payload) VALUES (19793, 'v19793');
-- line 19794: deterministic comment
SELECT nested FROM t WHERE id IN (19795, 19796, 19797);
INSERT INTO bench_t_84 (id, payload) VALUES (19796, 'v19796');
/* block header 19797 */
/* block header 19798 */
SELECT `mysql_19799` FROM `tbl_49`;
BEGIN; SELECT 19800; COMMIT;
SELECT nested FROM t WHERE id IN (19801, 19802, 19803);
INSERT INTO bench_t_90 (id, payload) VALUES (19802, 'v19802');
SELECT * FROM "quoted_19803" WHERE col = E'esc\'19803';
SELECT 19804 AS id, 'row_19804' AS label;
$dz$ dollar body 19805 ; semicolon inside $dz$
-- line 19806: deterministic comment
BEGIN; SELECT 19807; COMMIT;
DELETE FROM bench_t_0 WHERE id = 0;
UPDATE bench_t_33 SET payload = 19809 WHERE id = 1;
/* block header 19810 */
SELECT `mysql_19811` FROM `tbl_11`;
# hash comment 19812
SELECT `mysql_19813` FROM `tbl_13`;
WITH cte_19814 AS (SELECT 19814 AS n) SELECT n FROM cte_19814;
DELETE FROM bench_t_7 WHERE id = 7;
BEGIN; SELECT 19816; COMMIT;
-- line 19817: deterministic comment
SELECT `mysql_19818` FROM `tbl_18`;
SELECT 19819 AS id, 'row_19819' AS label;
SELECT * FROM "quoted_19820" WHERE col = E'esc\'19820';
INSERT INTO bench_t_109 (id, payload) VALUES (19821, 'v19821');
WITH cte_19822 AS (SELECT 19822 AS n) SELECT n FROM cte_19822;
-- line 19823: deterministic comment
SELECT 19824 AS id, 'row_19824' AS label;
SELECT nested FROM t WHERE id IN (19825, 19826, 19827);
SELECT nested FROM t WHERE id IN (19826, 19827, 19828);
WITH cte_19827 AS (SELECT 19827 AS n) SELECT n FROM cte_19827;
/* block header 19828 */
WITH cte_19829 AS (SELECT 19829 AS n) SELECT n FROM cte_19829;
/* block header 19830 */
SELECT * FROM "quoted_19831" WHERE col = E'esc\'19831';
# hash comment 19832
SELECT 19833 AS id, 'row_19833' AS label;
INSERT INTO bench_t_122 (id, payload) VALUES (19834, 'v19834');
SELECT nested FROM t WHERE id IN (19835, 19836, 19837);
WITH cte_19836 AS (SELECT 19836 AS n) SELECT n FROM cte_19836;
SELECT * FROM "quoted_19837" WHERE col = E'esc\'19837';
BEGIN; SELECT 19838; COMMIT;
SELECT 19839 AS id, 'row_19839' AS label;
SELECT nested FROM t WHERE id IN (19840, 19841, 19842);
SELECT [bracket_19841] FROM [dbo].[tbl_1];
SELECT [bracket_19842] FROM [dbo].[tbl_2];
SELECT nested FROM t WHERE id IN (19843, 19844, 19845);
$dz$ dollar body 19844 ; semicolon inside $dz$
WITH cte_19845 AS (SELECT 19845 AS n) SELECT n FROM cte_19845;
BEGIN; SELECT 19846; COMMIT;
SELECT 19847 AS id, 'row_19847' AS label;
SELECT * FROM "quoted_19848" WHERE col = E'esc\'19848';
SELECT [bracket_19849] FROM [dbo].[tbl_9];
SELECT nested FROM t WHERE id IN (19850, 19851, 19852);
$dz$ dollar body 19851 ; semicolon inside $dz$
SELECT [bracket_19852] FROM [dbo].[tbl_12];
# hash comment 19853
SELECT nested FROM t WHERE id IN (19854, 19855, 19856);
UPDATE bench_t_15 SET payload = 19855 WHERE id = 15;
SELECT * FROM "quoted_19856" WHERE col = E'esc\'19856';
BEGIN; SELECT 19857; COMMIT;
# hash comment 19858
BEGIN; SELECT 19859; COMMIT;
WITH cte_19860 AS (SELECT 19860 AS n) SELECT n FROM cte_19860;
SELECT 19861 AS id, 'row_19861' AS label;
UPDATE bench_t_22 SET payload = 19862 WHERE id = 22;
UPDATE bench_t_23 SET payload = 19863 WHERE id = 23;
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
# hash comment 19865
BEGIN; SELECT 19866; COMMIT;
INSERT INTO bench_t_27 (id, payload) VALUES (19867, 'v19867');
SELECT nested FROM t WHERE id IN (19868, 19869, 19870);
SELECT * FROM "quoted_19869" WHERE col = E'esc\'19869';
SELECT nested FROM t WHERE id IN (19870, 19871, 19872);
DELETE FROM bench_t_31 WHERE id = 15;
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT 19873 AS id, 'row_19873' AS label;
INSERT INTO bench_t_34 (id, payload) VALUES (19874, 'v19874');
WITH cte_19875 AS (SELECT 19875 AS n) SELECT n FROM cte_19875;
BEGIN; SELECT 19876; COMMIT;
SELECT [bracket_19877] FROM [dbo].[tbl_37];
INSERT INTO bench_t_38 (id, payload) VALUES (19878, 'v19878');
SELECT `mysql_19879` FROM `tbl_29`;
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT 19881 AS id, 'row_19881' AS label;
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
BEGIN; SELECT 19884; COMMIT;
WITH cte_19885 AS (SELECT 19885 AS n) SELECT n FROM cte_19885;
WITH cte_19886 AS (SELECT 19886 AS n) SELECT n FROM cte_19886;
INSERT INTO bench_t_47 (id, payload) VALUES (19887, 'v19887');
BEGIN; SELECT 19888; COMMIT;
UPDATE bench_t_49 SET payload = 19889 WHERE id = 17;
-- line 19890: deterministic comment
SELECT CASE WHEN 1 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT [bracket_19892] FROM [dbo].[tbl_12];
SELECT `mysql_19893` FROM `tbl_43`;
SELECT `mysql_19894` FROM `tbl_44`;
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
BEGIN; SELECT 19896; COMMIT;
INSERT INTO bench_t_57 (id, payload) VALUES (19897, 'v19897');
UPDATE bench_t_58 SET payload = 19898 WHERE id = 26;
SELECT * FROM "quoted_19899" WHERE col = E'esc\'19899';
$dz$ dollar body 19900 ; semicolon inside $dz$
SELECT * FROM "quoted_19901" WHERE col = E'esc\'19901';
# hash comment 19902
UPDATE bench_t_63 SET payload = 19903 WHERE id = 31;
/* block header 19904 */
/* block header 19905 */
UPDATE bench_t_2 SET payload = 19906 WHERE id = 2;
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 2 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT * FROM "quoted_19908" WHERE col = E'esc\'19908';
WITH cte_19909 AS (SELECT 19909 AS n) SELECT n FROM cte_19909;
INSERT INTO bench_t_70 (id, payload) VALUES (19910, 'O''Brien');
-- line 19911: deterministic comment
WITH cte_19912 AS (SELECT 19912 AS n) SELECT n FROM cte_19912;
/* block header 19913 */
/* block header 19914 */
SELECT `mysql_19915` FROM `tbl_15`;
# hash comment 19916
UPDATE bench_t_13 SET payload = 19917 WHERE id = 13;
BEGIN; SELECT 19918; COMMIT;
SELECT * FROM "quoted_19919" WHERE col = E'esc\'19919';
SELECT nested FROM t WHERE id IN (19920, 19921, 19922);
BEGIN; SELECT 19921; COMMIT;
INSERT INTO bench_t_82 (id, payload) VALUES (19922, 'v19922');
/* block header 19923 */
# hash comment 19924
SELECT [bracket_19925] FROM [dbo].[tbl_5];
WITH cte_19926 AS (SELECT 19926 AS n) SELECT n FROM cte_19926;
WITH cte_19927 AS (SELECT 19927 AS n) SELECT n FROM cte_19927;
SELECT * FROM "quoted_19928" WHERE col = E'esc\'19928';
SELECT nested FROM t WHERE id IN (19929, 19930, 19931);
/* block header 19930 */
WITH cte_19931 AS (SELECT 19931 AS n) SELECT n FROM cte_19931;
SELECT CASE WHEN 2 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
DELETE FROM bench_t_29 WHERE id = 13;
/* block header 19934 */
SELECT 19935 AS id, 'row_19935' AS label;
SELECT `mysql_19936` FROM `tbl_36`;
INSERT INTO bench_t_97 (id, payload) VALUES (19937, 'v19937');
SELECT * FROM "quoted_19938" WHERE col = E'esc\'19938';
WITH cte_19939 AS (SELECT 19939 AS n) SELECT n FROM cte_19939;
DELETE FROM bench_t_4 WHERE id = 4;
INSERT INTO bench_t_101 (id, payload) VALUES (19941, 'v19941');
UPDATE bench_t_38 SET payload = 19942 WHERE id = 6;
UPDATE bench_t_39 SET payload = 19943 WHERE id = 7;
BEGIN; SELECT 19944; COMMIT;
INSERT INTO bench_t_105 (id, payload) VALUES (19945, 'v19945');
INSERT INTO bench_t_106 (id, payload) VALUES (19946, 'v19946');
-- line 19947: deterministic comment
DELETE FROM bench_t_12 WHERE id = 12;
DELETE FROM bench_t_13 WHERE id = 13;
BEGIN; SELECT 19950; COMMIT;
/* block header 19951 */
SELECT [bracket_19952] FROM [dbo].[tbl_32];
UPDATE bench_t_49 SET payload = 19953 WHERE id = 17;
SELECT * FROM "quoted_19954" WHERE col = E'esc\'19954';
SELECT 19955 AS id, 'row_19955' AS label;
SELECT * FROM "quoted_19956" WHERE col = E'esc\'19956';
SELECT `mysql_19957` FROM `tbl_7`;
UPDATE bench_t_54 SET payload = 19958 WHERE id = 22;
BEGIN; SELECT 19959; COMMIT;
BEGIN; SELECT 19960; COMMIT;
DELETE FROM bench_t_25 WHERE id = 9;
UPDATE bench_t_58 SET payload = 19962 WHERE id = 26;
SELECT nested FROM t WHERE id IN (19963, 19964, 19965);
UPDATE bench_t_60 SET payload = 19964 WHERE id = 28;
BEGIN; SELECT 19965; COMMIT;
INSERT INTO bench_t_126 (id, payload) VALUES (19966, 'v19966');
SELECT * FROM "quoted_19967" WHERE col = E'esc\'19967';
SELECT CASE WHEN 3 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT 19969 AS id, 'row_19969' AS label;
WITH cte_19970 AS (SELECT 19970 AS n) SELECT n FROM cte_19970;
UPDATE bench_t_3 SET payload = 19971 WHERE id = 3;
SELECT `mysql_19972` FROM `tbl_22`;
BEGIN; SELECT 19973; COMMIT;
SELECT [bracket_19974] FROM [dbo].[tbl_14];
SELECT 19975 AS id, 'row_19975' AS label;
BEGIN; SELECT 19976; COMMIT;
SELECT * FROM "quoted_19977" WHERE col = E'esc\'19977';
SELECT nested FROM t WHERE id IN (19978, 19979, 19980);
SELECT 19979 AS id, 'row_19979' AS label;
SELECT CASE WHEN 0 = 0 THEN 'a' WHEN 0 = 0 THEN 'b' ELSE 'c' END AS bucket;
SELECT `mysql_19981` FROM `tbl_31`;
BEGIN; SELECT 19982; COMMIT;
/* block header 19983 */
WITH cte_19984 AS (SELECT 19984 AS n) SELECT n FROM cte_19984;
SELECT 19985 AS id, 'row_19985' AS label;
UPDATE bench_t_18 SET payload = 19986 WHERE id = 18;
SELECT * FROM "quoted_19987" WHERE col = E'esc\'19987';
SELECT [bracket_19988] FROM [dbo].[tbl_28];
-- line 19989: deterministic comment
SELECT [bracket_19990] FROM [dbo].[tbl_30];
WITH cte_19991 AS (SELECT 19991 AS n) SELECT n FROM cte_19991;
/* block header 19992 */
BEGIN; SELECT 19993; COMMIT;
DELETE FROM bench_t_26 WHERE id = 10;
-- line 19995: deterministic comment
DELETE FROM bench_t_28 WHERE id = 12;
INSERT INTO bench_t_29 (id, payload) VALUES (19997, 'v19997');
# hash comment 19998
SELECT CASE WHEN 4 = 0 THEN 'a' WHEN 1 = 0 THEN 'b' ELSE 'c' END AS bucket;
