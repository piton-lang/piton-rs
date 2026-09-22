# Escaping

## Description

Escaping works a little differently in Piton than other languages.  To escape special characters, you simply wrap them in backslashes.  So for example \ {1 + 2 + 3} \ would become { 1 + 2 + 3 }.
You can stack backslashes to escape backslashes themselves:
\\\ \\ \ {1 + 2 + 3} \ \\ \\\ would become \\ \ {1 + 2 + 3} \ \\.

## Multi Line

Multi-line escape blocks are valid.
