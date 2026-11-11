# Plugin Rendering

## Description

A plugin that writes documents gets the compiler's own renderers from the host instead of reimplementing them, so its output matches what `piton compile` and `piton slice` print, and the rules for writing Piton as Markdown stay in one place.

## Markdown

Markdown is rendered by the Markdown renderer described in the compilation specification: an anchor as a document, properties as sections, or a value as inline text.
References are links, and where a link goes depends on where each anchor was written. So a plugin first describes its documents: each output path, with the anchors it holds in order, and each anchor written to a file of its own, without headings, like a skill's SKILL.md. From that the host builds a link space.
In a link space, each document is rendered with its references linked relative to the file being written. A reference to an anchor lands on that anchor's heading in the file it is in, and a reference to a property on the property's heading; headings that repeat in a file get distinct fragments, the way a Markdown viewer numbers them. A reference to an anchor nothing wrote is written as its name, not a link. The plugin asks the link space for the same document, sections or text, from the path it is writing, and gets the same bytes the compiler would write there.

## Placeholders

A plugin can declare placeholders: exported names in its Piton files whose value is only known when its output is written, like a path relative to the file being written. Piton evaluates each to a marker that can't collide with prose, and the plugin replaces the marker in each file it plans. The host gives the plugin each placeholder's marker, and a function that writes a path relative to the file being written, so a placeholder resolves the same way a link does.

## Encoders

YAML for frontmatter, a TOML string, and a JSON string, encoded by real encoders, so text with quotes, colons or new lines is always valid where it lands.

## Paths

The host writes paths with forward slashes on every platform, and computes a path from one file in the project to another, so links and placeholders are the same on every machine.
