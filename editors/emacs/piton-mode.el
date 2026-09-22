;;; piton-mode.el --- Major mode for Piton  -*- lexical-binding: t; -*-

;; Version: 0.1.0
;; Package-Requires: ((emacs "29.1"))
;; Keywords: languages
;; SPDX-License-Identifier: MIT OR Apache-2.0

;;; Commentary:

;; Piton is whitespace-structured and most of its content is prose, so this
;; mode highlights the structure and leaves the prose alone.  The spec lists
;; Emacs as using tree-sitter and LSP together: `piton-ts-mode' uses the
;; grammar in ../tree-sitter-piton when it is installed, and both modes talk to
;; `piton lsp' through eglot for everything beyond highlighting.

;;; Code:

(require 'treesit nil t)

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
turned on.  The formatter leaves commented content alone."
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

(defun piton--indent-line ()
  "Indent to a multiple of four, which is the only width the language uses."
  (let* ((above-indent (save-excursion
                         (forward-line -1)
                         (current-indentation)))
         (blank-above (save-excursion
                        (forward-line -1)
                        (looking-at "[ \t]*$")))
         (opens-block (save-excursion
                        (forward-line -1)
                        (looking-at ".*:[ \t]*$")))
         (target (cond
                  ;; Enter on a blank line inside a dictionary or anchor
                  ;; leaves the block: one level back, bottoming at the
                  ;; margin.  The language server answers the same move in
                  ;; `on_type_formatting'.
                  ((and blank-above (> above-indent 0))
                   (- above-indent 4))
                  (opens-block (+ above-indent 4))
                  (t above-indent))))
    (indent-line-to (max 0 target))))

(defun piton--format-on-save ()
  "Format the buffer before saving, when `piton-format-on-save' is non-nil.
Formatting goes through the attached server, which leaves commented
content alone."
  (when (and piton-format-on-save
             (bound-and-true-p eglot--managed-mode)
             (fboundp 'eglot-format-buffer))
    (eglot-format-buffer)))

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
  ;; A blank line is an explicit line break inside a string, so refilling prose
  ;; would change the value.
  (setq-local fill-paragraph-function #'ignore))

;;;###autoload
(when (and (fboundp 'treesit-available-p) (treesit-available-p))
  (define-derived-mode piton-ts-mode piton-mode "Piton[ts]"
    "Major mode for editing Piton sources, using tree-sitter."
    (when (treesit-ready-p 'piton)
      (treesit-parser-create 'piton)
      (setq-local font-lock-defaults nil)
      (treesit-major-mode-setup))))

;;;###autoload
(add-to-list 'auto-mode-alist '("\\.pi\\'" . piton-mode))

(with-eval-after-load 'eglot
  ;; Everything beyond highlighting needs the resolved program, so it comes
  ;; from the server.
  (add-to-list 'eglot-server-programs
               `(piton-mode . (,piton-executable "lsp")))
  (add-to-list 'eglot-server-programs
               `(piton-ts-mode . (,piton-executable "lsp")))
  ;; Saving formats only when the option is on; `piton-ts-mode' derives from
  ;; `piton-mode', so this hook covers both.
  (add-hook 'piton-mode-hook #'piton--format-on-save))

(provide 'piton-mode)
;;; piton-mode.el ends here
