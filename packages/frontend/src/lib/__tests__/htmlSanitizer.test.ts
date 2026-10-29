import { describe, expect, it } from 'vitest';
import { sanitizeHtml } from '../htmlSanitizer';

describe('htmlSanitizer', () => {
  it('passes safe HTML through', () => {
    const input = '<p>Hello <strong>world</strong></p>';
    expect(sanitizeHtml(input)).toContain('Hello');
    expect(sanitizeHtml(input)).toContain('<strong>');
  });

  it('strips script tags', () => {
    const input = '<p>Hello</p><script>alert("xss")</script><p>World</p>';
    const result = sanitizeHtml(input);
    expect(result).not.toContain('<script');
    expect(result).toContain('Hello');
    expect(result).toContain('World');
  });

  it('strips iframe tags', () => {
    const input = '<iframe src="evil.com"></iframe>';
    expect(sanitizeHtml(input)).not.toContain('<iframe');
  });

  it('strips event handler attributes', () => {
    const input = '<p onclick="alert(1)">Hello</p>';
    const result = sanitizeHtml(input);
    expect(result).not.toContain('onclick');
    expect(result).toContain('Hello');
  });

  it('neuters javascript: links', () => {
    const input = '<a href="javascript:alert(1)">Click</a>';
    const result = sanitizeHtml(input);
    expect(result).not.toContain('javascript:');
    expect(result).toContain('href="#"');
    expect(result).toContain('Click');
  });

  it('adds target="_blank" and rel="noopener noreferrer" to links', () => {
    const input = '<a href="https://example.com">Link</a>';
    const result = sanitizeHtml(input);
    expect(result).toContain('target="_blank"');
    expect(result).toContain('rel="noopener noreferrer"');
  });

  it('strips style tags', () => {
    const input = '<style>body { color: red; }</style><p>Hello</p>';
    const result = sanitizeHtml(input);
    expect(result).not.toContain('<style');
    expect(result).toContain('Hello');
  });

  it('strips form/input elements', () => {
    const input = '<form><input type="text" /></form>';
    expect(sanitizeHtml(input)).not.toContain('<form');
    expect(sanitizeHtml(input)).not.toContain('<input');
  });

  it('handles nested dangerous content', () => {
    const input = '<div><p>Hello</p><script>evil()</script><p>World</p></div>';
    const result = sanitizeHtml(input);
    expect(result).not.toContain('<script');
    expect(result).toContain('Hello');
    expect(result).toContain('World');
  });

  it('preserves markdown-rendered HTML structures', () => {
    const input =
      '<h1>Title</h1><ul><li>Item 1</li><li>Item 2</li></ul><pre><code>const x = 1;</code></pre>';
    const result = sanitizeHtml(input);
    expect(result).toContain('<h1>');
    expect(result).toContain('<ul>');
    expect(result).toContain('<pre>');
  });

  it('strips onmouseover and other event handlers', () => {
    const input = '<div onmouseover="alert(1)" onfocus="alert(2)">hover</div>';
    const result = sanitizeHtml(input);
    expect(result).not.toContain('onmouseover');
    expect(result).not.toContain('onfocus');
    expect(result).toContain('hover');
  });

  it('handles javascript: in href with various casing', () => {
    const input = '<a href="JAVASCRIPT:alert(1)">XSS</a>';
    const result = sanitizeHtml(input);
    expect(result).not.toContain('javascript:');
    expect(result).not.toContain('JAVASCRIPT:');
  });

  it('[tester] strips svg tags', () => {
    const input = '<svg onload="alert(1)"><circle r="50"/></svg><p>Safe</p>';
    const result = sanitizeHtml(input);
    expect(result).not.toContain('<svg');
    expect(result).toContain('Safe');
  });

  it('[tester] strips math tags', () => {
    const input = '<math><mi>x</mi></math><p>Safe</p>';
    const result = sanitizeHtml(input);
    expect(result).not.toContain('<math');
    expect(result).toContain('Safe');
  });

  it('[tester] strips noscript tags', () => {
    const input = '<noscript><img src=x onerror=alert(1)></noscript><p>Safe</p>';
    const result = sanitizeHtml(input);
    expect(result).not.toContain('<noscript');
    expect(result).toContain('Safe');
  });

  it('[tester] strips base tags', () => {
    const input = '<base href="evil.com"><p>Safe</p>';
    const result = sanitizeHtml(input);
    expect(result).not.toContain('<base');
    expect(result).toContain('Safe');
  });

  it('[tester] strips link tags', () => {
    const input = '<link rel="stylesheet" href="evil.css"><p>Safe</p>';
    const result = sanitizeHtml(input);
    expect(result).not.toContain('<link');
    expect(result).toContain('Safe');
  });

  it('[tester] strips meta tags', () => {
    const input = '<meta http-equiv="refresh" content="0;url=evil.com"><p>Safe</p>';
    const result = sanitizeHtml(input);
    expect(result).not.toContain('<meta');
    expect(result).toContain('Safe');
  });

  it('[tester] handles deeply nested dangerous content', () => {
    const input = '<div><div><div><script>evil()</script></div></div></div>';
    const result = sanitizeHtml(input);
    expect(result).not.toContain('<script');
  });

  it('[tester] preserves safe href links', () => {
    const input = '<a href="https://example.com">Link</a>';
    const result = sanitizeHtml(input);
    expect(result).toContain('href="https://example.com"');
    expect(result).toContain('target="_blank"');
  });

  it('[tester] handles empty input', () => {
    expect(sanitizeHtml('')).toBe('');
  });

  it('[tester] strips button tags', () => {
    const input = '<button onclick="alert(1)">Click</button>';
    const result = sanitizeHtml(input);
    expect(result).not.toContain('<button');
  });

  it('[tester] strips textarea and select tags', () => {
    const input = '<textarea>text</textarea><select><option>opt</option></select>';
    const result = sanitizeHtml(input);
    expect(result).not.toContain('<textarea');
    expect(result).not.toContain('<select');
  });

  it('[tester] handles javascript: with whitespace variations', () => {
    const input = '<a href="  javascript:alert(1)">XSS</a>';
    const result = sanitizeHtml(input);
    expect(result).not.toContain('javascript:');
    expect(result).toContain('href="#"');
  });
});
