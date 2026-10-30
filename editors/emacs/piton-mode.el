;;; piton-mode.el --- Major mode for Piton  -*- lexical-binding: t; -*-

;; Version: 0.1.0
;; Package-Requires: ((emacs "29.1"))
;; Keywords: languages
;; SPDX-License-Identifier: MIT OR Apache-2.0

;;; Commentary:

;; The spec lists Emacs as using tree-sitter and LSP together.
;; `piton-ts-mode' highlights with the grammar in ../tree-sitter-piton, reading
;; the grammar's own `highlights.scm' (shipped next to this file as
;; queries/highlights.scm, a byte-identical copy) rather than a second set of
;; rules, so Emacs colours a file exactly as Zed, Helix and Neovim do.  Both
;; modes talk to `piton lsp' through eglot for everything beyond highlighting.
;;
;; `piton-mode' on its own does not highlight.  There used to be a set of
;; regexps here, and they were a seventh approximation of the parser that
;; nothing tested; without the grammar, the mode provides indentation and
;; comments, and says once that highlighting needs it.
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

(defvar piton-mode-syntax-table
  (let ((table (make-syntax-table)))
    ;; A slash is punctuation: `//' only opens a comment at the start of a
    ;; line, which the syntax table cannot say, so `piton--syntax-propertize'
    ;; marks those.  The newline ends the comment.
    (modify-syntax-entry ?/ "." table)
    (modify-syntax-entry ?\n ">" table)
    (modify-syntax-entry ?_ "w" table)
    (modify-syntax-entry ?- "_" table)
    (modify-syntax-entry ?\" "\"" table)
    table)
  "Syntax table for `piton-mode'.")

;;;; Comments

;; A comment has to be on its own line: a line whose first non-blank
;; characters are `//'.  At the end of a line of code `//' is just text, so
;; `url: https://example.com // note' is the string
;; `https://example.com // note'.  The lines of an escape block (between two
;; lines of the same run of backslashes) are literal, so a `//' line inside
;; one is not a comment either.  A code fence protects nothing: the compiler
;; reads a `//' line inside one as a comment, and so does this.

(defconst piton--escape-open-regexp "[ \t]*\\(\\\\+\\)[ \t]*$"
  "A line of backslashes, which opens or closes an escape block.")

(defun piton--verbatim-step (close)
  "The literal-block state after the line at point.
CLOSE is the regexp for the line that ends the escape block the line at
point is in, or nil when it is in none.  Return the same for the next
line."
  (cond
   (close (if (looking-at close) nil close))
   ((looking-at piton--escape-open-regexp)
    (concat "[ \t]*" (regexp-quote (match-string 1)) "[ \t]*$"))
   (t nil)))

(defun piton--verbatim-state-at (pos)
  "The literal-block state at the start of the line holding POS.
See `piton--verbatim-step'."
  (save-excursion
    (let ((limit (progn (goto-char pos) (line-beginning-position)))
          (close nil))
      (goto-char (point-min))
      (while (< (point) limit)
        (setq close (piton--verbatim-step close))
        (forward-line 1))
      close)))

(defun piton--syntax-propertize (start end)
  "Mark the whole-line comments between START and END.
The first slash of a line's leading `//' gets comment-start syntax,
unless the line is inside an escape block."
  (goto-char start)
  (beginning-of-line)
  (let ((close (piton--verbatim-state-at (point))))
    (while (< (point) end)
      (when (and (not close) (looking-at "[ \t]*\\(/\\)/"))
        (put-text-property (match-beginning 1) (match-end 1)
                           'syntax-table (string-to-syntax "<")))
      (setq close (piton--verbatim-step close))
      (unless (zerop (forward-line 1))
        (goto-char (point-max))))))

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
list item the same way, and a comment is not code.  A comment is a whole
line: at the end of a line of code `//' is text, so `key: // note:' is a
key with a value, and opens nothing.  None of them opens a
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

(defvar piton--highlighting-notice-shown nil
  "Whether this session has been told highlighting needs the grammar.")

(defun piton--notice-no-highlighting (buffer)
  "Say once that BUFFER is not highlighted without the tree-sitter grammar."
  (when (and (buffer-live-p buffer)
             (not piton--highlighting-notice-shown)
             (with-current-buffer buffer
               (not (and (eq major-mode 'piton-ts-mode)
                         (fboundp 'treesit-ready-p)
                         (treesit-ready-p 'piton t)))))
    (setq piton--highlighting-notice-shown t)
    (message "Piton: highlighting needs the tree-sitter grammar; see `piton-ts-mode'")))


;;;###autoload
(define-derived-mode piton-mode prog-mode "Piton"
  "Major mode for editing Piton sources."
  :syntax-table piton-mode-syntax-table
  ;; No highlighting without the grammar: see the commentary.  KEYWORDS-ONLY
  ;; with no keywords, so the syntax table does not colour quoted prose as a
  ;; string either.
  (setq-local font-lock-defaults '(nil t))
  (setq-local syntax-propertize-function #'piton--syntax-propertize)
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
  (add-hook 'before-save-hook #'piton--format-on-save nil t)
  ;; Checked once the mode (or `piton-ts-mode', which runs this body first)
  ;; has finished setting up.
  (unless piton--highlighting-notice-shown
    (run-with-idle-timer 0 nil #'piton--notice-no-highlighting (current-buffer))))

;;;; Tree-sitter

;; The faces each capture in highlights.scm is shown in.  The captures are the
;; canonical tree-sitter names in editors/fixtures/scopes.toml and these faces
;; are its `emacs' column; crates/piton-syntax/tests/editors.rs checks that
;; every capture the query uses has an entry here.
(defconst piton-ts-capture-faces
  '(("comment" . font-lock-comment-face)
    ("keyword" . font-lock-keyword-face)
    ("type.definition" . font-lock-type-face)
    ("type" . font-lock-type-face)
    ("variable" . font-lock-variable-use-face)
    ("property" . font-lock-property-name-face)
    ("string.special.path" . font-lock-string-face)
    ("type.builtin" . font-lock-builtin-face)
    ("number" . font-lock-number-face)
    ("constant.builtin" . font-lock-constant-face)
    ("string" . font-lock-string-face)
    ("variable.builtin" . font-lock-builtin-face)
    ("operator" . font-lock-operator-face)
    ("punctuation.list_marker" . font-lock-misc-punctuation-face)
    ("punctuation.special" . font-lock-misc-punctuation-face)
    ("string.escape" . font-lock-escape-face)
    ("string.special" . font-lock-string-face))
  "Face for each capture name in the shared highlights query.")

(defconst piton-ts--highlights-file
  (expand-file-name "queries/highlights.scm"
                    (file-name-directory (or load-file-name buffer-file-name)))
  "The grammar's highlights query, shipped next to this file.")

(defun piton-ts--highlights-query ()
  "The shared highlights query with each capture rewritten to its face.
Emacs reads a capture name as the face to use, where other editors read it
as a theme key, so `@keyword' becomes `@font-lock-keyword-face'.  A capture
with no entry in `piton-ts-capture-faces' is dropped rather than guessed."
  (with-temp-buffer
    (insert-file-contents piton-ts--highlights-file)
    (goto-char (point-min))
    (while (re-search-forward "@\\([A-Za-z_][A-Za-z0-9_.]*\\)" nil t)
      (let ((face (cdr (assoc (match-string 1) piton-ts-capture-faces))))
        (replace-match (if face (concat "@" (symbol-name face)) "") t t)))
    (buffer-string)))

(defvar piton-ts--font-lock-settings nil
  "Tree-sitter font-lock settings for `piton-ts-mode', built on first use.")

(defun piton-ts--font-lock-settings ()
  "Font-lock rules read from the grammar's own highlights query."
  (or piton-ts--font-lock-settings
      (setq piton-ts--font-lock-settings
            (treesit-font-lock-rules
             :language 'piton
             :feature 'piton
             ;; No node is captured twice, so the order of the patterns does
             ;; not matter; an inner capture overrides an outer one.
             :override t
             (piton-ts--highlights-query)))))

;;;###autoload
(when (and (fboundp 'treesit-available-p) (treesit-available-p))
  (define-derived-mode piton-ts-mode piton-mode "Piton[ts]"
    "Major mode for editing Piton sources, using tree-sitter for highlighting.
Without the grammar installed this is `piton-mode', which does not
highlight.
Indentation stays with `piton--indent-line': the grammar is line-oriented
and cannot express the Enter rules."
    (when (treesit-ready-p 'piton)
      (treesit-parser-create 'piton)
      (setq-local treesit-font-lock-settings (piton-ts--font-lock-settings))
      (setq-local treesit-font-lock-feature-list '((piton)))
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
