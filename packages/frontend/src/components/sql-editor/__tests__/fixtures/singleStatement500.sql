SELECT
  base.id,
  base.name,
  base.created_at,
  CASE WHEN base.flag_0 = 0 THEN 'v0' ELSE NULL END AS dyn_0,
  CASE WHEN base.flag_1 = 1 THEN 'v1' ELSE NULL END AS dyn_1,
  CASE WHEN base.flag_2 = 2 THEN 'v2' ELSE NULL END AS dyn_2,
  CASE WHEN base.flag_3 = 0 THEN 'v3' ELSE NULL END AS dyn_3,
  CASE WHEN base.flag_4 = 1 THEN 'v4' ELSE NULL END AS dyn_4,
  CASE WHEN base.flag_5 = 2 THEN 'v5' ELSE NULL END AS dyn_5,
  CASE WHEN base.flag_6 = 0 THEN 'v6' ELSE NULL END AS dyn_6,
  CASE WHEN base.flag_7 = 1 THEN 'v7' ELSE NULL END AS dyn_7,
  CASE WHEN base.flag_8 = 2 THEN 'v8' ELSE NULL END AS dyn_8,
  CASE WHEN base.flag_9 = 0 THEN 'v9' ELSE NULL END AS dyn_9,
  CASE WHEN base.flag_10 = 1 THEN 'v10' ELSE NULL END AS dyn_10,
  CASE WHEN base.flag_11 = 2 THEN 'v11' ELSE NULL END AS dyn_11,
  CASE WHEN base.flag_0 = 0 THEN 'v12' ELSE NULL END AS dyn_12,
  CASE WHEN base.flag_1 = 1 THEN 'v13' ELSE NULL END AS dyn_13,
  CASE WHEN base.flag_2 = 2 THEN 'v14' ELSE NULL END AS dyn_14,
  CASE WHEN base.flag_3 = 0 THEN 'v15' ELSE NULL END AS dyn_15,
  CASE WHEN base.flag_4 = 1 THEN 'v16' ELSE NULL END AS dyn_16,
  CASE WHEN base.flag_5 = 2 THEN 'v17' ELSE NULL END AS dyn_17,
  CASE WHEN base.flag_6 = 0 THEN 'v18' ELSE NULL END AS dyn_18,
  CASE WHEN base.flag_7 = 1 THEN 'v19' ELSE NULL END AS dyn_19,
  CASE WHEN base.flag_8 = 2 THEN 'v20' ELSE NULL END AS dyn_20,
  CASE WHEN base.flag_9 = 0 THEN 'v21' ELSE NULL END AS dyn_21,
  CASE WHEN base.flag_10 = 1 THEN 'v22' ELSE NULL END AS dyn_22,
  CASE WHEN base.flag_11 = 2 THEN 'v23' ELSE NULL END AS dyn_23,
  CASE WHEN base.flag_0 = 0 THEN 'v24' ELSE NULL END AS dyn_24,
  CASE WHEN base.flag_1 = 1 THEN 'v25' ELSE NULL END AS dyn_25,
  CASE WHEN base.flag_2 = 2 THEN 'v26' ELSE NULL END AS dyn_26,
  CASE WHEN base.flag_3 = 0 THEN 'v27' ELSE NULL END AS dyn_27,
  CASE WHEN base.flag_4 = 1 THEN 'v28' ELSE NULL END AS dyn_28,
  CASE WHEN base.flag_5 = 2 THEN 'v29' ELSE NULL END AS dyn_29,
  CASE WHEN base.flag_6 = 0 THEN 'v30' ELSE NULL END AS dyn_30,
  CASE WHEN base.flag_7 = 1 THEN 'v31' ELSE NULL END AS dyn_31,
  CASE WHEN base.flag_8 = 2 THEN 'v32' ELSE NULL END AS dyn_32,
  CASE WHEN base.flag_9 = 0 THEN 'v33' ELSE NULL END AS dyn_33,
  CASE WHEN base.flag_10 = 1 THEN 'v34' ELSE NULL END AS dyn_34,
  CASE WHEN base.flag_11 = 2 THEN 'v35' ELSE NULL END AS dyn_35,
  CASE WHEN base.flag_0 = 0 THEN 'v36' ELSE NULL END AS dyn_36,
  CASE WHEN base.flag_1 = 1 THEN 'v37' ELSE NULL END AS dyn_37,
  CASE WHEN base.flag_2 = 2 THEN 'v38' ELSE NULL END AS dyn_38,
  CASE WHEN base.flag_3 = 0 THEN 'v39' ELSE NULL END AS dyn_39,
  CASE WHEN base.flag_4 = 1 THEN 'v40' ELSE NULL END AS dyn_40,
  CASE WHEN base.flag_5 = 2 THEN 'v41' ELSE NULL END AS dyn_41,
  CASE WHEN base.flag_6 = 0 THEN 'v42' ELSE NULL END AS dyn_42,
  CASE WHEN base.flag_7 = 1 THEN 'v43' ELSE NULL END AS dyn_43,
  CASE WHEN base.flag_8 = 2 THEN 'v44' ELSE NULL END AS dyn_44,
  CASE WHEN base.flag_9 = 0 THEN 'v45' ELSE NULL END AS dyn_45,
  CASE WHEN base.flag_10 = 1 THEN 'v46' ELSE NULL END AS dyn_46,
  CASE WHEN base.flag_11 = 2 THEN 'v47' ELSE NULL END AS dyn_47,
  CASE WHEN base.flag_0 = 0 THEN 'v48' ELSE NULL END AS dyn_48,
  CASE WHEN base.flag_1 = 1 THEN 'v49' ELSE NULL END AS dyn_49,
  CASE WHEN base.flag_2 = 2 THEN 'v50' ELSE NULL END AS dyn_50,
  CASE WHEN base.flag_3 = 0 THEN 'v51' ELSE NULL END AS dyn_51,
  CASE WHEN base.flag_4 = 1 THEN 'v52' ELSE NULL END AS dyn_52,
  CASE WHEN base.flag_5 = 2 THEN 'v53' ELSE NULL END AS dyn_53,
  CASE WHEN base.flag_6 = 0 THEN 'v54' ELSE NULL END AS dyn_54,
  CASE WHEN base.flag_7 = 1 THEN 'v55' ELSE NULL END AS dyn_55,
  CASE WHEN base.flag_8 = 2 THEN 'v56' ELSE NULL END AS dyn_56,
  CASE WHEN base.flag_9 = 0 THEN 'v57' ELSE NULL END AS dyn_57,
  CASE WHEN base.flag_10 = 1 THEN 'v58' ELSE NULL END AS dyn_58,
  CASE WHEN base.flag_11 = 2 THEN 'v59' ELSE NULL END AS dyn_59,
  CASE WHEN base.flag_0 = 0 THEN 'v60' ELSE NULL END AS dyn_60,
  CASE WHEN base.flag_1 = 1 THEN 'v61' ELSE NULL END AS dyn_61,
  CASE WHEN base.flag_2 = 2 THEN 'v62' ELSE NULL END AS dyn_62,
  CASE WHEN base.flag_3 = 0 THEN 'v63' ELSE NULL END AS dyn_63,
  CASE WHEN base.flag_4 = 1 THEN 'v64' ELSE NULL END AS dyn_64,
  CASE WHEN base.flag_5 = 2 THEN 'v65' ELSE NULL END AS dyn_65,
  CASE WHEN base.flag_6 = 0 THEN 'v66' ELSE NULL END AS dyn_66,
  CASE WHEN base.flag_7 = 1 THEN 'v67' ELSE NULL END AS dyn_67,
  CASE WHEN base.flag_8 = 2 THEN 'v68' ELSE NULL END AS dyn_68,
  CASE WHEN base.flag_9 = 0 THEN 'v69' ELSE NULL END AS dyn_69,
  CASE WHEN base.flag_10 = 1 THEN 'v70' ELSE NULL END AS dyn_70,
  CASE WHEN base.flag_11 = 2 THEN 'v71' ELSE NULL END AS dyn_71,
  CASE WHEN base.flag_0 = 0 THEN 'v72' ELSE NULL END AS dyn_72,
  CASE WHEN base.flag_1 = 1 THEN 'v73' ELSE NULL END AS dyn_73,
  CASE WHEN base.flag_2 = 2 THEN 'v74' ELSE NULL END AS dyn_74,
  CASE WHEN base.flag_3 = 0 THEN 'v75' ELSE NULL END AS dyn_75,
  CASE WHEN base.flag_4 = 1 THEN 'v76' ELSE NULL END AS dyn_76,
  CASE WHEN base.flag_5 = 2 THEN 'v77' ELSE NULL END AS dyn_77,
  CASE WHEN base.flag_6 = 0 THEN 'v78' ELSE NULL END AS dyn_78,
  CASE WHEN base.flag_7 = 1 THEN 'v79' ELSE NULL END AS dyn_79,
  CASE WHEN base.flag_8 = 2 THEN 'v80' ELSE NULL END AS dyn_80,
  CASE WHEN base.flag_9 = 0 THEN 'v81' ELSE NULL END AS dyn_81,
  CASE WHEN base.flag_10 = 1 THEN 'v82' ELSE NULL END AS dyn_82,
  CASE WHEN base.flag_11 = 2 THEN 'v83' ELSE NULL END AS dyn_83,
  CASE WHEN base.flag_0 = 0 THEN 'v84' ELSE NULL END AS dyn_84,
  CASE WHEN base.flag_1 = 1 THEN 'v85' ELSE NULL END AS dyn_85,
  CASE WHEN base.flag_2 = 2 THEN 'v86' ELSE NULL END AS dyn_86,
  CASE WHEN base.flag_3 = 0 THEN 'v87' ELSE NULL END AS dyn_87,
  CASE WHEN base.flag_4 = 1 THEN 'v88' ELSE NULL END AS dyn_88,
  CASE WHEN base.flag_5 = 2 THEN 'v89' ELSE NULL END AS dyn_89,
  CASE WHEN base.flag_6 = 0 THEN 'v90' ELSE NULL END AS dyn_90,
  CASE WHEN base.flag_7 = 1 THEN 'v91' ELSE NULL END AS dyn_91,
  CASE WHEN base.flag_8 = 2 THEN 'v92' ELSE NULL END AS dyn_92,
  CASE WHEN base.flag_9 = 0 THEN 'v93' ELSE NULL END AS dyn_93,
  CASE WHEN base.flag_10 = 1 THEN 'v94' ELSE NULL END AS dyn_94,
  CASE WHEN base.flag_11 = 2 THEN 'v95' ELSE NULL END AS dyn_95,
  CASE WHEN base.flag_0 = 0 THEN 'v96' ELSE NULL END AS dyn_96,
  CASE WHEN base.flag_1 = 1 THEN 'v97' ELSE NULL END AS dyn_97,
  CASE WHEN base.flag_2 = 2 THEN 'v98' ELSE NULL END AS dyn_98,
  CASE WHEN base.flag_3 = 0 THEN 'v99' ELSE NULL END AS dyn_99,
  CASE WHEN base.flag_4 = 1 THEN 'v100' ELSE NULL END AS dyn_100,
  CASE WHEN base.flag_5 = 2 THEN 'v101' ELSE NULL END AS dyn_101,
  CASE WHEN base.flag_6 = 0 THEN 'v102' ELSE NULL END AS dyn_102,
  CASE WHEN base.flag_7 = 1 THEN 'v103' ELSE NULL END AS dyn_103,
  CASE WHEN base.flag_8 = 2 THEN 'v104' ELSE NULL END AS dyn_104,
  CASE WHEN base.flag_9 = 0 THEN 'v105' ELSE NULL END AS dyn_105,
  CASE WHEN base.flag_10 = 1 THEN 'v106' ELSE NULL END AS dyn_106,
  CASE WHEN base.flag_11 = 2 THEN 'v107' ELSE NULL END AS dyn_107,
  CASE WHEN base.flag_0 = 0 THEN 'v108' ELSE NULL END AS dyn_108,
  CASE WHEN base.flag_1 = 1 THEN 'v109' ELSE NULL END AS dyn_109,
  CASE WHEN base.flag_2 = 2 THEN 'v110' ELSE NULL END AS dyn_110,
  CASE WHEN base.flag_3 = 0 THEN 'v111' ELSE NULL END AS dyn_111,
  CASE WHEN base.flag_4 = 1 THEN 'v112' ELSE NULL END AS dyn_112,
  CASE WHEN base.flag_5 = 2 THEN 'v113' ELSE NULL END AS dyn_113,
  CASE WHEN base.flag_6 = 0 THEN 'v114' ELSE NULL END AS dyn_114,
  CASE WHEN base.flag_7 = 1 THEN 'v115' ELSE NULL END AS dyn_115,
  CASE WHEN base.flag_8 = 2 THEN 'v116' ELSE NULL END AS dyn_116,
  CASE WHEN base.flag_9 = 0 THEN 'v117' ELSE NULL END AS dyn_117,
  CASE WHEN base.flag_10 = 1 THEN 'v118' ELSE NULL END AS dyn_118,
  CASE WHEN base.flag_11 = 2 THEN 'v119' ELSE NULL END AS dyn_119,
  agg.cnt,
  agg.total
FROM (
  SELECT
    t.id,
    t.name,
    t.created_at,
    t.flag_0,
    t.flag_1,
    t.flag_2,
    t.flag_3,
    t.flag_4,
    t.flag_5,
    t.flag_6,
    t.flag_7,
    t.flag_8,
    t.flag_9,
    t.flag_10,
    t.flag_11,
    t.flag_0,
    t.flag_1,
    t.flag_2,
    t.flag_3,
    t.flag_4,
    t.flag_5,
    t.flag_6,
    t.flag_7,
    t.flag_8,
    t.flag_9,
    t.flag_10,
    t.flag_11,
    t.flag_0,
    t.flag_1,
    t.flag_2,
    t.flag_3,
    t.flag_4,
    t.flag_5,
    t.flag_6,
    t.flag_7,
    t.flag_8,
    t.flag_9,
    t.flag_10,
    t.flag_11,
    t.flag_0,
    t.flag_1,
    t.flag_2,
    t.flag_3,
    t.flag_4,
    t.flag_5,
    t.flag_6,
    t.flag_7,
    t.flag_8,
    t.flag_9,
    t.flag_10,
    t.flag_11,
    t.flag_0,
    t.flag_1,
    t.flag_2,
    t.flag_3,
    t.flag_4,
    t.flag_5,
    t.flag_6,
    t.flag_7,
    t.flag_8,
    t.flag_9,
    t.flag_10,
    t.flag_11,
    t.flag_0,
    t.flag_1,
    t.flag_2,
    t.flag_3,
    t.flag_4,
    t.flag_5,
    t.flag_6,
    t.flag_7,
    t.flag_8,
    t.flag_9,
    t.flag_10,
    t.flag_11,
    t.flag_0,
    t.flag_1,
    t.flag_2,
    t.flag_3,
    t.flag_4,
    t.flag_5,
    t.flag_6,
    t.flag_7,
    ROW_NUMBER() OVER (PARTITION BY t.category ORDER BY t.created_at DESC) AS rn
  FROM source_table t
  WHERE t.deleted_at IS NULL
    AND t.category IN (
      'cat_0',
      'cat_1',
      'cat_2',
      'cat_3',
      'cat_4',
      'cat_5',
      'cat_6',
      'cat_7',
      'cat_8',
      'cat_9',
      'cat_10',
      'cat_11',
      'cat_12',
      'cat_13',
      'cat_14',
      'cat_15',
      'cat_16',
      'cat_17',
      'cat_18',
      'cat_19',
      'cat_20',
      'cat_21',
      'cat_22',
      'cat_23',
      'cat_24',
      'cat_25',
      'cat_26',
      'cat_27',
      'cat_28',
      'cat_29',
      'cat_30',
      'cat_31',
      'cat_32',
      'cat_33',
      'cat_34',
      'cat_35',
      'cat_36',
      'cat_37',
      'cat_38',
      'cat_39',
      'cat_40',
      'cat_41',
      'cat_42',
      'cat_43',
      'cat_44',
      'cat_45',
      'cat_46',
      'cat_47',
      'cat_48',
      'cat_49',
      'cat_50',
      'cat_51',
      'cat_52',
      'cat_53',
      'cat_54',
      'cat_55',
      'cat_56',
      'cat_57',
      'cat_58',
      'cat_59'
    )
) base
LEFT JOIN (
  SELECT ref_0.parent_id, COUNT(*) AS cnt_0, SUM(ref_0.amount) AS total_0
  FROM ref_table_0 ref_0
  WHERE ref_0.status = 'active'
  GROUP BY ref_0.parent_id
  UNION ALL
  SELECT ref_1.parent_id, COUNT(*) AS cnt_1, SUM(ref_1.amount) AS total_1
  FROM ref_table_1 ref_1
  WHERE ref_1.status = 'active'
  GROUP BY ref_1.parent_id
  UNION ALL
  SELECT ref_2.parent_id, COUNT(*) AS cnt_2, SUM(ref_2.amount) AS total_2
  FROM ref_table_2 ref_2
  WHERE ref_2.status = 'active'
  GROUP BY ref_2.parent_id
  UNION ALL
  SELECT ref_3.parent_id, COUNT(*) AS cnt_3, SUM(ref_3.amount) AS total_3
  FROM ref_table_3 ref_3
  WHERE ref_3.status = 'active'
  GROUP BY ref_3.parent_id
  UNION ALL
  SELECT ref_4.parent_id, COUNT(*) AS cnt_4, SUM(ref_4.amount) AS total_4
  FROM ref_table_4 ref_4
  WHERE ref_4.status = 'active'
  GROUP BY ref_4.parent_id
  UNION ALL
  SELECT ref_5.parent_id, COUNT(*) AS cnt_5, SUM(ref_5.amount) AS total_5
  FROM ref_table_5 ref_5
  WHERE ref_5.status = 'active'
  GROUP BY ref_5.parent_id
  UNION ALL
  SELECT ref_6.parent_id, COUNT(*) AS cnt_6, SUM(ref_6.amount) AS total_6
  FROM ref_table_6 ref_6
  WHERE ref_6.status = 'active'
  GROUP BY ref_6.parent_id
  UNION ALL
  SELECT ref_7.parent_id, COUNT(*) AS cnt_7, SUM(ref_7.amount) AS total_7
  FROM ref_table_7 ref_7
  WHERE ref_7.status = 'active'
  GROUP BY ref_7.parent_id
  UNION ALL
  SELECT ref_8.parent_id, COUNT(*) AS cnt_8, SUM(ref_8.amount) AS total_8
  FROM ref_table_8 ref_8
  WHERE ref_8.status = 'active'
  GROUP BY ref_8.parent_id
  UNION ALL
  SELECT ref_9.parent_id, COUNT(*) AS cnt_9, SUM(ref_9.amount) AS total_9
  FROM ref_table_9 ref_9
  WHERE ref_9.status = 'active'
  GROUP BY ref_9.parent_id
  UNION ALL
  SELECT ref_10.parent_id, COUNT(*) AS cnt_10, SUM(ref_10.amount) AS total_10
  FROM ref_table_10 ref_10
  WHERE ref_10.status = 'active'
  GROUP BY ref_10.parent_id
  UNION ALL
  SELECT ref_11.parent_id, COUNT(*) AS cnt_11, SUM(ref_11.amount) AS total_11
  FROM ref_table_11 ref_11
  WHERE ref_11.status = 'active'
  GROUP BY ref_11.parent_id
  UNION ALL
  SELECT ref_12.parent_id, COUNT(*) AS cnt_12, SUM(ref_12.amount) AS total_12
  FROM ref_table_12 ref_12
  WHERE ref_12.status = 'active'
  GROUP BY ref_12.parent_id
  UNION ALL
  SELECT ref_13.parent_id, COUNT(*) AS cnt_13, SUM(ref_13.amount) AS total_13
  FROM ref_table_13 ref_13
  WHERE ref_13.status = 'active'
  GROUP BY ref_13.parent_id
  UNION ALL
  SELECT ref_14.parent_id, COUNT(*) AS cnt_14, SUM(ref_14.amount) AS total_14
  FROM ref_table_14 ref_14
  WHERE ref_14.status = 'active'
  GROUP BY ref_14.parent_id
  UNION ALL
  SELECT ref_15.parent_id, COUNT(*) AS cnt_15, SUM(ref_15.amount) AS total_15
  FROM ref_table_15 ref_15
  WHERE ref_15.status = 'active'
  GROUP BY ref_15.parent_id
  UNION ALL
  SELECT ref_16.parent_id, COUNT(*) AS cnt_16, SUM(ref_16.amount) AS total_16
  FROM ref_table_16 ref_16
  WHERE ref_16.status = 'active'
  GROUP BY ref_16.parent_id
  UNION ALL
  SELECT ref_17.parent_id, COUNT(*) AS cnt_17, SUM(ref_17.amount) AS total_17
  FROM ref_table_17 ref_17
  WHERE ref_17.status = 'active'
  GROUP BY ref_17.parent_id
  UNION ALL
  SELECT ref_18.parent_id, COUNT(*) AS cnt_18, SUM(ref_18.amount) AS total_18
  FROM ref_table_18 ref_18
  WHERE ref_18.status = 'active'
  GROUP BY ref_18.parent_id
  UNION ALL
  SELECT ref_19.parent_id, COUNT(*) AS cnt_19, SUM(ref_19.amount) AS total_19
  FROM ref_table_19 ref_19
  WHERE ref_19.status = 'active'
  GROUP BY ref_19.parent_id
  UNION ALL
  SELECT ref_20.parent_id, COUNT(*) AS cnt_20, SUM(ref_20.amount) AS total_20
  FROM ref_table_20 ref_20
  WHERE ref_20.status = 'active'
  GROUP BY ref_20.parent_id
  UNION ALL
  SELECT ref_21.parent_id, COUNT(*) AS cnt_21, SUM(ref_21.amount) AS total_21
  FROM ref_table_21 ref_21
  WHERE ref_21.status = 'active'
  GROUP BY ref_21.parent_id
  UNION ALL
  SELECT ref_22.parent_id, COUNT(*) AS cnt_22, SUM(ref_22.amount) AS total_22
  FROM ref_table_22 ref_22
  WHERE ref_22.status = 'active'
  GROUP BY ref_22.parent_id
  UNION ALL
  SELECT ref_23.parent_id, COUNT(*) AS cnt_23, SUM(ref_23.amount) AS total_23
  FROM ref_table_23 ref_23
  WHERE ref_23.status = 'active'
  GROUP BY ref_23.parent_id
  UNION ALL
  SELECT ref_24.parent_id, COUNT(*) AS cnt_24, SUM(ref_24.amount) AS total_24
  FROM ref_table_24 ref_24
  WHERE ref_24.status = 'active'
  GROUP BY ref_24.parent_id
  UNION ALL
  SELECT ref_25.parent_id, COUNT(*) AS cnt_25, SUM(ref_25.amount) AS total_25
  FROM ref_table_25 ref_25
  WHERE ref_25.status = 'active'
  GROUP BY ref_25.parent_id
  UNION ALL
  SELECT ref_26.parent_id, COUNT(*) AS cnt_26, SUM(ref_26.amount) AS total_26
  FROM ref_table_26 ref_26
  WHERE ref_26.status = 'active'
  GROUP BY ref_26.parent_id
  UNION ALL
  SELECT ref_27.parent_id, COUNT(*) AS cnt_27, SUM(ref_27.amount) AS total_27
  FROM ref_table_27 ref_27
  WHERE ref_27.status = 'active'
  GROUP BY ref_27.parent_id
  UNION ALL
  SELECT ref_28.parent_id, COUNT(*) AS cnt_28, SUM(ref_28.amount) AS total_28
  FROM ref_table_28 ref_28
  WHERE ref_28.status = 'active'
  GROUP BY ref_28.parent_id
  UNION ALL
  SELECT ref_29.parent_id, COUNT(*) AS cnt_29, SUM(ref_29.amount) AS total_29
  FROM ref_table_29 ref_29
  WHERE ref_29.status = 'active'
  GROUP BY ref_29.parent_id
  UNION ALL
  SELECT ref_30.parent_id, COUNT(*) AS cnt_30, SUM(ref_30.amount) AS total_30
  FROM ref_table_30 ref_30
  WHERE ref_30.status = 'active'
  GROUP BY ref_30.parent_id
  UNION ALL
  SELECT ref_31.parent_id, COUNT(*) AS cnt_31, SUM(ref_31.amount) AS total_31
  FROM ref_table_31 ref_31
  WHERE ref_31.status = 'active'
  GROUP BY ref_31.parent_id
  UNION ALL
  SELECT ref_32.parent_id, COUNT(*) AS cnt_32, SUM(ref_32.amount) AS total_32
  FROM ref_table_32 ref_32
  WHERE ref_32.status = 'active'
  GROUP BY ref_32.parent_id
  UNION ALL
  SELECT ref_33.parent_id, COUNT(*) AS cnt_33, SUM(ref_33.amount) AS total_33
  FROM ref_table_33 ref_33
  WHERE ref_33.status = 'active'
  GROUP BY ref_33.parent_id
  UNION ALL
  SELECT ref_34.parent_id, COUNT(*) AS cnt_34, SUM(ref_34.amount) AS total_34
  FROM ref_table_34 ref_34
  WHERE ref_34.status = 'active'
  GROUP BY ref_34.parent_id
  UNION ALL
  SELECT ref_35.parent_id, COUNT(*) AS cnt_35, SUM(ref_35.amount) AS total_35
  FROM ref_table_35 ref_35
  WHERE ref_35.status = 'active'
  GROUP BY ref_35.parent_id
  UNION ALL
  SELECT ref_36.parent_id, COUNT(*) AS cnt_36, SUM(ref_36.amount) AS total_36
  FROM ref_table_36 ref_36
  WHERE ref_36.status = 'active'
  GROUP BY ref_36.parent_id
  UNION ALL
  SELECT ref_37.parent_id, COUNT(*) AS cnt_37, SUM(ref_37.amount) AS total_37
  FROM ref_table_37 ref_37
  WHERE ref_37.status = 'active'
  GROUP BY ref_37.parent_id
  UNION ALL
  SELECT ref_38.parent_id, COUNT(*) AS cnt_38, SUM(ref_38.amount) AS total_38
  FROM ref_table_38 ref_38
  WHERE ref_38.status = 'active'
  GROUP BY ref_38.parent_id
  UNION ALL
  SELECT ref_39.parent_id, COUNT(*) AS cnt_39, SUM(ref_39.amount) AS total_39
  FROM ref_table_39 ref_39
  WHERE ref_39.status = 'active'
  GROUP BY ref_39.parent_id
) agg ON agg.parent_id = base.id
WHERE base.rn = 1
  AND base.flag_0 IS NOT NULL
  AND base.flag_1 IS NOT NULL
  AND base.flag_2 IS NOT NULL
  AND base.flag_3 IS NOT NULL
  AND base.flag_4 IS NOT NULL
  AND base.flag_5 IS NOT NULL
  AND base.flag_6 IS NOT NULL
  AND base.flag_7 IS NOT NULL
  AND base.flag_8 IS NOT NULL
  AND base.flag_9 IS NOT NULL
  AND base.flag_10 IS NOT NULL
  AND base.flag_11 IS NOT NULL
  AND base.flag_0 IS NOT NULL
  AND base.flag_1 IS NOT NULL
  AND base.flag_2 IS NOT NULL
  AND base.flag_3 IS NOT NULL
  AND base.flag_4 IS NOT NULL
  AND base.flag_5 IS NOT NULL
  AND base.flag_6 IS NOT NULL
  AND base.flag_7 IS NOT NULL
  AND base.flag_8 IS NOT NULL
  AND base.flag_9 IS NOT NULL
  AND base.flag_10 IS NOT NULL
  AND base.flag_11 IS NOT NULL
  AND base.flag_0 IS NOT NULL
  AND base.flag_1 IS NOT NULL
  AND base.flag_2 IS NOT NULL
  AND base.flag_3 IS NOT NULL
  AND base.flag_4 IS NOT NULL
  AND base.flag_5 IS NOT NULL
  AND base.flag_6 IS NOT NULL
  AND base.flag_7 IS NOT NULL
  AND base.flag_8 IS NOT NULL
  AND base.flag_9 IS NOT NULL
  AND base.flag_10 IS NOT NULL
  AND base.flag_11 IS NOT NULL
  AND base.flag_0 IS NOT NULL
  AND base.flag_1 IS NOT NULL
  AND base.flag_2 IS NOT NULL
  AND base.flag_3 IS NOT NULL
  AND base.flag_4 IS NOT NULL
  AND base.flag_5 IS NOT NULL
  AND base.flag_6 IS NOT NULL
  AND base.flag_7 IS NOT NULL
  AND base.flag_8 IS NOT NULL
  AND base.flag_9 IS NOT NULL
  AND base.flag_10 IS NOT NULL
  AND base.flag_11 IS NOT NULL
  AND base.flag_0 IS NOT NULL
  AND base.flag_1 IS NOT NULL
ORDER BY base.created_at DESC, base.id
LIMIT 1000
