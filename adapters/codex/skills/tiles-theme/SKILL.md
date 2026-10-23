---
name: tiles-theme
description: List Tiles' card themes or switch to one (obsidian, light, dark). Use when the user asks to see or change the Tiles theme, or types $tiles-theme.
---
Call the tiles_theme tool from the tiles MCP server. If the user named a theme (obsidian, light or dark), pass it as name. Otherwise call the tool with no arguments, show the themes it lists, and ask which one they want, then call it again with their pick. Tell the user what its result says.
