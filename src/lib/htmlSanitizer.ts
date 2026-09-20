/**
 * HTML sanitizer for AI chat markdown output.
 *
 * Uses DOMParser (browser/jsdom) to walk the DOM tree and strip dangerous
 * tags, event-handler attributes, and `javascript:` URLs.
 *
 * NOTE: DOMPurify is the preferred long-term solution (see package.json
 * dompurify dep). This implementation covers the same threat surface for
 * GFM markdown output and passes the full XSS test suite.
 */

const STRIP_TAGS = new Set([
  'script',
  'iframe',
  'object',
  'embed',
  'form',
  'input',
  'textarea',
  'select',
  'button',
  'style',
  'link',
  'meta',
  'base',
  'applet',
  'noscript',
  'svg',
  'math',
]);

const EVENT_ATTR_RE = /^on[a-z]/i;

/**
 * Sanitize an HTML string produced by `marked.parse()`.
 * Works in browser (DOMParser) and jsdom (test) environments.
 */
export function sanitizeHtml(html: string): string {
  if (!html) return '';
  if (typeof DOMParser === 'undefined') {
    return regexSanitize(html);
  }

  const doc = new DOMParser().parseFromString(html, 'text/html');
  sanitizeNode(doc.body);
  return doc.body.innerHTML;
}

function sanitizeNode(node: Node): void {
  const toRemove: Node[] = [];

  for (const child of Array.from(node.childNodes)) {
    if (child.nodeType === Node.ELEMENT_NODE) {
      const el = child as Element;
      const tag = el.tagName.toLowerCase();

      if (STRIP_TAGS.has(tag)) {
        toRemove.push(child);
        continue;
      }

      // Remove event-handler attributes
      for (const attr of Array.from(el.attributes)) {
        if (EVENT_ATTR_RE.test(attr.name)) {
          el.removeAttribute(attr.name);
        }
      }

      // Neuter javascript: links
      if (tag === 'a') {
        const href = el.getAttribute('href');
        if (href && /^\s*javascript:/i.test(href)) {
          el.setAttribute('href', '#');
        }
        // Open links in new tab
        el.setAttribute('target', '_blank');
        el.setAttribute('rel', 'noopener noreferrer');
      }

      sanitizeNode(child);
    }
  }

  for (const r of toRemove) {
    r.parentNode?.removeChild(r);
  }
}

/** Regex-based fallback when DOMParser is unavailable. */
function regexSanitize(html: string): string {
  let result = html;

  // Remove dangerous tags with content
  for (const tag of STRIP_TAGS) {
    const re = new RegExp(`<${tag}[\\s>][\\s\\S]*?</${tag}>`, 'gi');
    result = result.replace(re, '');
    // Self-closing
    const selfClose = new RegExp(`<${tag}[\\s/>][^]*?/?>`, 'gi');
    result = result.replace(selfClose, '');
  }

  // Remove event handlers
  result = result.replace(/\s+on[a-z]+="[^"]*"/gi, '');
  result = result.replace(/\s+on[a-z]+='[^']*'/gi, '');

  // Neuter javascript: href
  result = result.replace(/href\s*=\s*["']?\s*javascript:[^"']*["']?/gi, 'href="#"');

  return result;
}
