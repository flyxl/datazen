-- Durable fixture for manual Schema Tree checks.
-- Re-running this file preserves extra rows and only upserts the two named samples.
CREATE DATABASE IF NOT EXISTS `datazen_manual_schema_tree`
  CHARACTER SET utf8mb4 COLLATE utf8mb4_unicode_ci;
USE `datazen_manual_schema_tree`;

CREATE TABLE IF NOT EXISTS `manual_schema_tree_parents` (
  `id` INT NOT NULL PRIMARY KEY,
  `code` VARCHAR(64) NOT NULL UNIQUE
) ENGINE=InnoDB;

CREATE TABLE IF NOT EXISTS `manual_schema_tree_items` (
  `id` BIGINT NOT NULL AUTO_INCREMENT,
  `parent_id` INT NOT NULL,
  `code` VARCHAR(64) NOT NULL,
  `status` VARCHAR(16) NOT NULL,
  `created_at` TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
  `updated_at` TIMESTAMP NULL DEFAULT NULL,
  PRIMARY KEY (`id`),
  KEY `idx_manual_schema_tree_items_parent_status` (`parent_id`, `status`),
  CONSTRAINT `fk_manual_schema_tree_items_parent`
    FOREIGN KEY (`parent_id`) REFERENCES `manual_schema_tree_parents` (`id`)
) ENGINE=InnoDB;

INSERT INTO `manual_schema_tree_parents` (`id`, `code`) VALUES
  (1001, 'manual-parent-a'),
  (1002, 'manual-parent-b')
ON DUPLICATE KEY UPDATE `code` = VALUES(`code`);

INSERT INTO `manual_schema_tree_items` (`id`, `parent_id`, `code`, `status`) VALUES
  (2001, 1001, 'manual-active-a', 'active'),
  (2002, 1001, 'manual-inactive-a', 'inactive'),
  (2003, 1002, 'manual-active-b', 'active')
ON DUPLICATE KEY UPDATE
  `parent_id` = VALUES(`parent_id`),
  `code` = VALUES(`code`),
  `status` = VALUES(`status`);

CREATE OR REPLACE VIEW `manual_schema_tree_active_items` AS
SELECT `id`, `parent_id`, `code`, `status`, `created_at`, `updated_at`
FROM `manual_schema_tree_items`
WHERE `status` = 'active';

DROP FUNCTION IF EXISTS `manual_schema_tree_normalize_code`;
CREATE FUNCTION `manual_schema_tree_normalize_code` (`p_value` VARCHAR(64))
RETURNS VARCHAR(64)
DETERMINISTIC
RETURN UPPER(TRIM(`p_value`));

DROP PROCEDURE IF EXISTS `manual_schema_tree_list_children`;
CREATE PROCEDURE `manual_schema_tree_list_children` (IN `p_parent_id` INT)
SELECT `id`, `code`, `status`
FROM `manual_schema_tree_items`
WHERE `parent_id` = `p_parent_id`
ORDER BY `id`;

DROP TRIGGER IF EXISTS `manual_schema_tree_touch_updated_at`;
CREATE TRIGGER `manual_schema_tree_touch_updated_at`
BEFORE UPDATE ON `manual_schema_tree_items`
FOR EACH ROW SET NEW.`updated_at` = CURRENT_TIMESTAMP;
