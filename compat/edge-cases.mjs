export const edgeCases = [
  // Full-width digits: Unicode columns, delimiter widths, fences and line endings.
  { id: "no-full-width-number/unicode-prefix", content: "中文１２３\n" },
  { id: "no-full-width-number/emoji-prefix", content: "😀１２\n" },
  { id: "no-full-width-number/combining-prefix", content: "e\u0301１２\n" },
  { id: "no-full-width-number/bom-prefix", content: "\uFEFF版本１２\n" },
  { id: "no-full-width-number/multiple-runs", content: "１a２３b４\n" },
  {
    id: "no-full-width-number/double-backtick-code",
    content: "outside １２ ``inside ３４`` outside ５６\n"
  },
  {
    id: "no-full-width-number/tilde-fence",
    content: "~~~md\n１２３\n~~~\noutside ４５\n"
  },
  {
    id: "no-full-width-number/wide-backtick-fence",
    content: "````md\n１２\n```\n３４\n````\noutside ５６\n"
  },
  { id: "no-full-width-number/blockquote", content: "> text １２３\n" },
  { id: "no-full-width-number/crlf", content: "first\r\n版本１２３\r\n" },
  {
    id: "no-full-width-number/unmatched-backtick",
    content: "before `１２ after ３４\n"
  },

  // Empty inline code: matching runs, nesting, Unicode positions and newlines.
  { id: "no-empty-inline-code/single-space", content: "` `\n" },
  { id: "no-empty-inline-code/tab", content: "`\t`\n" },
  { id: "no-empty-inline-code/double-delimiter", content: "``  ``\n" },
  {
    id: "no-empty-inline-code/triple-delimiter",
    content: "before ```   ``` after\n"
  },
  {
    id: "no-empty-inline-code/quadruple-delimiter",
    content: "before ```` \t ```` after\n"
  },
  { id: "no-empty-inline-code/multiline", content: "before ` \n ` after\n" },
  {
    id: "no-empty-inline-code/multiple-spans",
    content: "` ` and ``  ``\n"
  },
  {
    id: "no-empty-inline-code/unicode-prefix",
    content: "中文 ` ` after\n"
  },
  {
    id: "no-empty-inline-code/emoji-prefix",
    content: "😀 ` ` after\n"
  },
  {
    id: "no-empty-inline-code/combining-prefix",
    content: "e\u0301 ` ` after\n"
  },
  { id: "no-empty-inline-code/blockquote", content: "> ` `\n" },
  { id: "no-empty-inline-code/list-item", content: "- ` `\n" },
  { id: "no-empty-inline-code/emphasis", content: "**` `**\n" },
  {
    id: "no-empty-inline-code/link-label",
    content: "[` `](https://example.com)\n"
  },
  {
    id: "no-empty-inline-code/mixed-unmatched-runs",
    content: "before ` after `` end\n"
  },
  {
    id: "no-empty-inline-code/tilde-fenced-content",
    content: "~~~md\n` `\n~~~\n"
  },
  {
    id: "no-empty-inline-code/crlf-position",
    content: "first\r\ntext ` `\r\n"
  },
  {
    id: "no-empty-inline-code/cr-position",
    content: "first\rtext ` `\r"
  },
  {
    id: "no-empty-inline-code/non-empty-same-width",
    content: "``value``\n"
  },

  // Blockquotes: indentation, nesting, interaction with other fixes and CRLF.
  {
    id: "no-multiple-space-blockquote/indented",
    content: "  >   text\n"
  },
  { id: "no-empty-blockquote/indented", content: "  >   \n" },
  { id: "no-multiple-space-blockquote/nested", content: ">> text\n" },
  {
    id: "no-empty-blockquote/with-blank-run",
    content: ">\n\n\ntext\n"
  },
  {
    id: "no-multiple-space-blockquote/ideographic-space",
    content: ">　１２\n"
  },
  {
    id: "no-multiple-space-blockquote/crlf",
    content: ">missing\r\n>   text\r\n"
  },
  {
    id: "no-multiple-space-blockquote/in-list",
    content: "- >missing\n"
  },

  // Blank lines: CR-only, mixed endings, fences and overlapping fixes.
  {
    id: "no-multiple-blank-lines/cr-only",
    content: "\rfirst\r\r\rsecond\r\r"
  },
  {
    id: "no-multiple-blank-lines/mixed-endings",
    content: "\nfirst\r\n\r\n\rsecond\n\n"
  },
  { id: "no-multiple-blank-lines/no-final-newline", content: "first" },
  { id: "no-multiple-blank-lines/one-final-newline", content: "first\n" },
  { id: "no-multiple-blank-lines/two-final-newlines", content: "first\n\n" },
  {
    id: "no-multiple-blank-lines/backtick-fence",
    content: "```text\nline\n\n\nline\n```\n"
  },
  {
    id: "no-multiple-blank-lines/tilde-fence",
    content: "~~~text\nline\n\n\nline\n~~~\n"
  },
  { id: "no-multiple-blank-lines/empty-document", content: "" },
  {
    id: "no-multiple-blank-lines/whitespace-crlf",
    content: " \r\n\t\r\n"
  },
  {
    id: "interactions/empty-blockquote-blank-lines-full-width",
    content: ">\n\n\n版本１２\n"
  },
  {
    id: "interactions/blockquote-spacing-blank-lines-full-width",
    content: ">   \n\n\n>missing １２\n"
  },
  {
    id: "no-multiple-blank-lines/bom-before-blank-run",
    content: "\uFEFF\n\ntext\n"
  },
  { id: "no-multiple-blank-lines/only-newlines", content: "\n\n" }
];
