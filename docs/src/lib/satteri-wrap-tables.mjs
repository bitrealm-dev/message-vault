/**
 * Wrap every Markdown table in `<div class="mv-table">`.
 *
 * The guidebook styles tables as a bordered, rounded box that scrolls
 * sideways on its own. A border with a radius does not clip a `<table>`
 * cleanly, so the box has to be a wrapper element, and Markdown cannot
 * write one. This is a Sätteri HTML-tree plugin (Astro's default Markdown
 * processor); it runs on both Markdown and MDX pages.
 */
export const wrapTables = {
  name: 'mv-wrap-tables',
  element: {
    filter: ['table'],
    visit(node, ctx) {
      ctx.wrapNode(node, {
        type: 'element',
        tagName: 'div',
        properties: { className: ['mv-table'] },
        children: [],
      });
    },
  },
};
