import { type Schema } from 'hast-util-sanitize';

// Extended schema that preserves standard HTML + KaTeX math elements
// Based on default GitHub schema plus KaTeX tags
export const sanitizeSchema: Schema = {
  tagNames: [
    // Standard HTML tags needed for markdown
    'h1', 'h2', 'h3', 'h4', 'h5', 'h6',
    'p', 'div', 'span', 'br', 'hr',
    'ul', 'ol', 'li',
    'blockquote', 'pre', 'code',
    'strong', 'em', 'b', 'i', 'u', 's',
    'a', 'img',
    'table', 'thead', 'tbody', 'tr', 'th', 'td',
    'sup', 'sub',
    // KaTeX math elements
    'math', 'semantics', 'annotation',
    'mrow', 'mi', 'mo', 'mn',
    'msup', 'msub', 'mfrac', 'mtd', 'mtr', 'mtable',
    'maligngroup', 'malignmark', 'mspace', 'msqrt', 'mroot',
    'mover', 'munder', 'munderover', 'mtext',
    'menclose', 'mpadded', 'mphantom',
    'mscarries', 'mscarry', 'msgroup', 'mlongdiv', 'msline',
    'dl', 'dd', 'dt',
    // rehype-pretty-code elements
    'figure', 'figcaption',
  ],
  attributes: {
    '*': ['className', 'class', 'id'],
    a: ['href', 'title', 'target', 'rel'],
    img: ['src', 'alt', 'title', 'width', 'height'],
    code: ['data-language', 'class'],
    pre: ['data-language', 'class', 'data-mermaid-source', 'data-mermaid-status'],
    figure: ['data-rehype-pretty-code-figure'],
    // KaTeX attributes
    math: ['xmlns', 'display'],
    semantics: ['xmlns'],
    annotation: ['encoding'],
    mspace: ['linebreak'],
    menclose: ['notation'],
    mpadded: ['lspace', 'rspace', 'width', 'height', 'depth'],
    maligngroup: ['align'],
    malignmark: ['edge'],
  },
  strip: ['script', 'style', 'iframe', 'object', 'embed'],
  clobber: ['a', 'img'],
  clobberPrefix: 'user-content-',
};

export default sanitizeSchema;
