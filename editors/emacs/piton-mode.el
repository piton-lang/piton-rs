;;; piton-mode.el --- Major mode for Piton  -*- lexical-binding: t; -*-

;; Version: 0.1.0
;; Package-Requires: ((emacs "29.1"))
;; Keywords: languages
;; SPDX-License-Identifier: MIT OR Apache-2.0

;;; Commentary:

;; Piton is whitespace-structured and most of its content is prose, so this
;; mode highlights the structure and leaves the prose alone.  The spec lists
;; Emacs as using tree-sitter and LSP together: `piton-ts-mode' highlights
;; with the grammar in ../tree-sitter-piton when it is installed, and both
;; modes talk to `piton lsp' through eglot for everything beyond highlighting.
;;
;; Editor behaviour, per the specification:
;;
;; - Enter on a colon line (a declaration, or a dictionary or anchor property
;;   with nothing after its colon) indents the new line one level.
;; - Enter on a blank line inside a dictionary or anchor dedents the new line
;;   one level.
;; - Autoformat on save is an option (`piton-format-on-save').

;;; Code:

(require 'treesit nil t)

(declare-function treesit-parser-create "treesit.c")
(declare-function treesit-ready-p "treesit")
(declare-function treesit-font-lock-rules "treesit")
(declare-function treesit-major-mode-setup "treesit")
(declare-function eglot-format-buffer "eglot")

(defgroup piton nil
  "Support for the Piton language."
  :group 'languages
  :prefix "piton-")

(defcustom piton-executable "piton"
  "Path to the piton binary.  The language server runs as `piton lsp'."
  :type 'string
  :group 'piton)

(defcustom piton-format-on-save nil
  "Whether to format the buffer through the language server before saving.

This is the specification's autoFormatOnSave rule: an option, off unless
turned on.  The formatter adds the space after `//' and touches nothing
else that is commented."
  :type 'boolean
  :group 'piton)

(defconst piton--declaration-keywords
  '("use" "from" "import" "export" "anchor" "abstract" "as" "extends" "pass")
  "Words that introduce or modify a declaration.")

(defconst piton--type-names
  '("string" "number" "boolean" "null" "list" "dictionary"
    "anchor" "reference" "any" "simple" "complex")
  "Type names usable in a `::' constraint.")

(defconst piton--constants '("true" "false" "null")
  "Literal values.")

(defvar piton-font-lock-keywords
  `(
    ;; A comment opens only after whitespace, so `https://x' is not one.
    ("\\(?:^\\|[ \t]\\)\\(//.*\\)$" 1 font-lock-comment-face)

    ;; Declarations: `[export] [abstract] <keyword> Name [as kw] [extends ...]:'
    ("^[ \t]*\\(?:\\(export\\)[ \t]+\\)?\\(?:\\(abstract\\)[ \t]+\\)?\\([a-z][a-z0-9-]*\\)[ \t]+\\([A-Za-z_][A-Za-z0-9_]*\\)"
     (1 font-lock-keyword-face nil t)
     (2 font-lock-keyword-face nil t)
     (3 font-lock-builtin-face)
     (4 font-lock-type-face))
    ("[ \t]\\(as\\)[ \t]+\\([a-z][a-z0-9-]*\\)"
     (1 font-lock-keyword-face)
     (2 font-lock-function-name-face))
    ("[ \t]\\(extends\\)[ \t]+" 1 font-lock-keyword-face)

    ;; Imports
    ("^[ \t]*\\(use\\|from\\)[ \t]+\\(\\S-+\\)"
     (1 font-lock-keyword-face)
     (2 font-lock-string-face))
    ("[ \t]\\(import\\|export\\)\\_>" 1 font-lock-keyword-face)

    ;; A key is any text without spaces followed by a colon.  Keywords are keys
    ;; here too: the colon is what separates the two readings.
    ("^[ \t]*\\([^ \t:]+\\)\\(?:::[^:\n]*\\)*:\\(?:[ \t]\\|$\\)" 1 font-lock-variable-name-face)
    (,(concat "::[ \t]*\\(?:extends[ \t]+\\)?\\(" (regexp-opt piton--type-names 'symbols) "\\)")
     1 font-lock-type-face)

    ;; `pass` is an intentionally empty body.
    ("^[ \t]*\\(pass\\)[ \t]*$" 1 font-lock-keyword-face)
    (,(regexp-opt piton--declaration-keywords 'symbols) . font-lock-keyword-face)

    ;; Structure markers
    ("^[ \t]*\\(-\\)\\(?:[ \t]\\|$\\)" 1 font-lock-negation-char-face)
    ("^[ \t]*\\(\\+\\+?\\)\\(?:[ \t]\\|$\\)" 1 font-lock-keyword-face)

    ;; Interpolation.  Four sigils: intrinsic, string, numeric, reference.
    ("\\(\\(?:\\$\\|#\\|@\\)?{\\)\\([^}\n]*\\)\\(}\\)"
     (1 font-lock-preprocessor-face)
     (2 font-lock-constant-face)
     (3 font-lock-preprocessor-face))

    ;; A line of backslashes delimits a literal block.
    ("^[ \t]*\\(\\\\+\\)[ \t]*$" 1 font-lock-preprocessor-face)
    (,(regexp-opt piton--constants 'symbols) . font-lock-constant-face)
    ("\\_<\\(this\\|self\\|super\\)\\_>" 1 font-lock-builtin-face))
  "Font lock rules for `piton-mode'.")

(defvar piton-mode-syntax-table
  (let ((table (make-syntax-table)))
    (modify-syntax-entry ?/ ". 12" table)
    (modify-syntax-entry ?\n ">" table)
    (modify-syntax-entry ?_ "w" table)
    (modify-syntax-entry ?- "_" table)
    (modify-syntax-entry ?\" "\"" table)
    table)
  "Syntax table for `piton-mode'.")

;;;; Indentation

(defconst piton--indent-width 4
  "Four spaces per level, not tabs, and not configurable.")

(defconst piton--block-opener-regexp
  (concat "\\`\\(?:"
          ;; A declaration at the margin: `export anchor A extends B:'.  It may
          ;; not start with `-', `+' or `/', which begin a list item, a merge
          ;; line or a comment.
          "[^-+/ \t\n][^:\n]*:[ \t]*"
          "\\|"
          ;; A key with nothing after its colon, possibly with a `::'
          ;; constraint chain: `frameworks:', `config:: dictionary:'.  A key
          ;; has no spaces, which is what keeps prose such as `For example:'
          ;; from opening a block.
          "[ \t]*[^-+:/ \t\n][^:/ \t\n]*\\(?:::[ \t]*[^:/ \t\n]+\\)*:[ \t]*"
          "\\)\\'")
  "A line that opens a block for the lines beneath it.

The same rule as the other editors' `increaseIndentPattern' and the
language server's `on_type_formatting'.")

(defun piton--opens-block-p (line)
  "Whether LINE ends by opening a block for the lines beneath it.

A list item is never a key, even with a colon at the end: `- Settings:'
is just the string `Settings:'.  A merge line (`+ ...', `++ ...') adds a
list item the same way, and a comment is not code.  None of them opens a
block.  Prose inside a string that happens to be a single word ending in
a colon cannot be told apart from a key without the parser, and is
treated as one."
  (let ((case-fold-search nil))
    (and (string-match-p ".*:[ \t]*$" line)
         (not (string-match-p "\\`[ \t]*\\(?:-\\|\\+\\+?\\)\\(?:[ \t]\\|\\'\\)" line))
         (not (string-match-p "\\`[ \t]*//" line))
         (string-match-p piton--block-opener-regexp line))))

(defun piton--line-string ()
  "The current line, without its newline."
  (buffer-substring-no-properties (line-beginning-position) (line-end-position)))

(defun piton--blank-line-p ()
  "Whether the current line is empty or whitespace only."
  (save-excursion
    (beginning-of-line)
    (looking-at-p "[ \t]*$")))

(defun piton--blank-depth ()
  "The depth of the blank line at point, as the Enter rules left it.

A blank line carries its depth as its indentation when it has any.  Emacs
usually strips it: `electric-indent-mode' deletes the whitespace left on a
line when Enter moves off it.  A blank with no indentation is then
measured by what precedes it.  The first blank after a line sits where
that line's next line would (one level in if it opens a block), and each
further blank in the run was dedented one more level by the blank-line
rule, so the run length recovers the depth."
  (if (> (current-indentation) 0)
      (current-indentation)
    ;; BLANKS counts the stripped blanks in the run, this one included; BASE
    ;; is the depth the first of them had.
    (let ((blanks 1)
          (base 0)
          (found nil))
      (save-excursion
        (while (and (not found) (zerop (forward-line -1)))
          (cond
           ((not (piton--blank-line-p))
            (setq found t
                  base (+ (current-indentation)
                          (if (piton--opens-block-p (piton--line-string))
                              piton--indent-width
                            0))))
           ((> (current-indentation) 0)
            ;; An indented blank further up kept its depth; the blank after
            ;; it was one level back.
            (setq found t
                  base (- (current-indentation) piton--indent-width)))
           (t (setq blanks (1+ blanks))))))
      (max 0 (- base (* piton--indent-width (1- blanks)))))))

(defun piton--wanted-indent ()
  "The indentation the current line should have."
  (let ((blank (piton--blank-line-p)))
    (save-excursion
      (beginning-of-line)
      (cond
       ((bobp) 0)
       ;; A line being typed.
       (blank
        (forward-line -1)
        (cond
         ;; Enter on a blank line inside a dictionary or anchor leaves the
         ;; block: one level back, bottoming at the margin.
         ((piton--blank-line-p)
          (max 0 (- (piton--blank-depth) piton--indent-width)))
         ;; Enter on a colon line lands one level in.
         ((piton--opens-block-p (piton--line-string))
          (+ (current-indentation) piton--indent-width))
         (t (current-indentation))))
       ;; A line with content: one level in under a line that opens a block,
       ;; else level with the line above.  After a blank line, where the
       ;; block ended is not something the text can say, so the line keeps
       ;; its own level, no deeper than the last line with content.
       (t
        (let ((own (current-indentation))
              (after-blank (save-excursion
                             (forward-line -1)
                             (piton--blank-line-p))))
          (skip-chars-backward " \t\n")
          (beginning-of-line)
          (let ((above (current-indentation)))
            (cond
             ((piton--blank-line-p) own)
             ((piton--opens-block-p (piton--line-string))
              (+ above piton--indent-width))
             (after-blank (min own above))
             (t above)))))))))

(defun piton--indent-line ()
  "Indent the current line according to Piton's Enter rules."
  (let ((target (piton--wanted-indent))
        (offset (- (current-column) (current-indentation))))
    (indent-line-to (max 0 target))
    (when (> offset 0)
      (move-to-column (+ (current-indentation) offset)))))

;;;; Formatting

(defun piton--format-on-save ()
  "Format the buffer before saving, when `piton-format-on-save' is non-nil.
Formatting goes through the attached eglot server, which adds the space
after `//' and touches nothing else that is commented."
  (when (and piton-format-on-save
             (bound-and-true-p eglot--managed-mode)
             (fboundp 'eglot-format-buffer))
    (eglot-format-buffer)))

;;;; Modes

;;;###autoload
(define-derived-mode piton-mode prog-mode "Piton"
  "Major mode for editing Piton sources."
  :syntax-table piton-mode-syntax-table
  (setq-local font-lock-defaults '(piton-font-lock-keywords))
  (setq-local comment-start "// ")
  (setq-local comment-start-skip "//+[ \t]*")
  (setq-local comment-end "")
  ;; Four spaces per level, not tabs, and not configurable.
  (setq-local indent-tabs-mode nil)
  (setq-local tab-width 4)
  (setq-local indent-line-function #'piton--indent-line)
  ;; Where a block ends cannot be read back from the text, so re-indenting
  ;; the line Enter left behind would only guess.  Like `python-mode', ask
  ;; `electric-indent-mode' to indent the new line and leave the old one.
  (setq-local electric-indent-inhibit t)
  ;; A blank line is an explicit line break inside a string, so refilling prose
  ;; would change the value.
  (setq-local fill-paragraph-function #'ignore)
  ;; Autoformat on save is an option: the hook is buffer-local and does
  ;; nothing unless `piton-format-on-save' is non-nil at save time.
  (add-hook 'before-save-hook #'piton--format-on-save nil t))

;;;; Tree-sitter

(defvar piton-ts--font-lock-settings nil
  "Tree-sitter font-lock settings for `piton-ts-mode', built on first use.")

(defun piton-ts--font-lock-settings ()
  "Font-lock rules mirroring ../tree-sitter-piton/queries/highlights.scm."
  (or piton-ts--font-lock-settings
      (setq piton-ts--font-lock-settings
            (treesit-font-lock-rules
             :language 'piton
             :feature 'comment
             '((comment) @font-lock-comment-face)

             :language 'piton
             :feature 'string
             '((text) @font-lock-string-face
               (string) @font-lock-string-face
               (fence) @font-lock-string-face
               (escape_block) @font-lock-string-face
               (module_path) @font-lock-string-face)

             :language 'piton
             :feature 'keyword
             :override t
             '(["export" "abstract" "as" "extends" "use" "from" "import"]
               @font-lock-keyword-face
               (pass_statement) @font-lock-keyword-face
               (from_declaration direction: _ @font-lock-keyword-face)
               (anchor_declaration
                keyword: (declaration_keyword) @font-lock-keyword-face))

             :language 'piton
             :feature 'definition
             :override t
             '((anchor_declaration name: (identifier) @font-lock-type-face)
               ;; A user keyword is sugar for `extends'.
               (anchor_declaration alias: (keyword_name) @font-lock-function-name-face)
               (base_list (identifier) @font-lock-type-face)
               (import_item name: (identifier) @font-lock-variable-name-face)
               (import_item alias: (identifier) @font-lock-variable-name-face))

             :language 'piton
             :feature 'property
             :override t
             '((property name: (key) @font-lock-property-name-face))

             :language 'piton
             :feature 'type
             :override t
             '((builtin_type) @font-lock-type-face
               (type_constraint type: (identifier) @font-lock-type-face)
               (type_constraint "extends" @font-lock-keyword-face)
               (type_constraint ":" @font-lock-delimiter-face)
               (list_suffix) @font-lock-bracket-face)

             :language 'piton
             :feature 'structure
             :override t
             '((list_item "-" @font-lock-punctuation-face)
               (merge_item operator: _ @font-lock-operator-face)
               (fence_marker) @font-lock-punctuation-face
               (fence_language) @font-lock-preprocessor-face
               (escape_marker) @font-lock-punctuation-face)

             :language 'piton
             :feature 'interpolation
             :override t
             '((interpolation (sigil) @font-lock-preprocessor-face)
               (interpolation "}" @font-lock-preprocessor-face)
               (constant) @font-lock-constant-face
               (self_reference) @font-lock-builtin-face
               (number) @font-lock-number-face
               (operator) @font-lock-operator-face
               ((identifier) @font-lock-type-face
                (:match "\\`[A-Z]" @font-lock-type-face)))))))

;;;###autoload
(when (and (fboundp 'treesit-available-p) (treesit-available-p))
  (define-derived-mode piton-ts-mode piton-mode "Piton[ts]"
    "Major mode for editing Piton sources, using tree-sitter for highlighting.
Falls back to `piton-mode' highlighting when the grammar is not installed.
Indentation stays with `piton--indent-line': the grammar is line-oriented
and cannot express the Enter rules."
    (when (treesit-ready-p 'piton)
      (treesit-parser-create 'piton)
      (setq-local treesit-font-lock-settings (piton-ts--font-lock-settings))
      (setq-local treesit-font-lock-feature-list
                  '((comment definition)
                    (keyword string property type)
                    (structure interpolation)))
      (treesit-major-mode-setup)
      ;; `treesit-major-mode-setup' leaves indentation alone when there are no
      ;; `treesit-simple-indent-rules', but make the intent explicit.
      (setq-local indent-line-function #'piton--indent-line))))

;;;###autoload
(add-to-list 'auto-mode-alist '("\\.pi\\'" . piton-mode))

(with-eval-after-load 'eglot
  ;; Everything beyond highlighting needs the resolved program, so it comes
  ;; from the server.
  (add-to-list 'eglot-server-programs
               `((piton-mode piton-ts-mode) . (,piton-executable "lsp"))))

(provide 'piton-mode)
;;; piton-mode.el ends here
