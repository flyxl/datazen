export const GETTING_STARTED_QUERY_TITLE = 'getting_started.sql';

export const SAMPLE_GETTING_STARTED_SQL = `-- Welcome to DataZen! Quick Sales Analysis
SELECT
  p.category,
  COUNT(o.id) as total_orders,
  ROUND(SUM(o.total_amount), 2) as total_revenue
FROM products p
JOIN orders o ON p.id = o.product_id
GROUP BY p.category
ORDER BY total_revenue DESC;`;
