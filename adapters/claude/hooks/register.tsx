// Tiles for Claude Code (ADR-004): draws a tiles_chart result as the card
// itself, in place of the tool's result row.
//
// Claude Code 2.1.302 mod API (the plugin-authoring types, claude-code.d.ts):
// a `ui.render` hook on the `ToolResult` component receives the tool's name
// and output; `Image` takes `{ file, format: 'png' }`, the terminal reads the
// file itself, and where the terminal shows no pixels it draws `alt` dim in
// the picture's place. Image shows pixels in kitty and Ghostty today, so
// `tiles run claude` loads this mod there and uses the wrapper elsewhere.

import type { Register } from 'claude-code'

import { cardFrom } from './card'

const TOOL = 'mcp__tiles__tiles_chart'

export const register: Register = on => {
  on('ui.render', { component: 'ToolResult' }, async ($, e, next) => {
    if (e.props.tool !== TOOL || e.props.isErrored) {
      return next(e)
    }
    const card = cardFrom(e.props.output)
    if (card === null) {
      return next(e)
    }
    const { Box, Image } = $.ui.resolve(e)
    return (
      <Box flexDirection="column">
        <Image
          key={card.id}
          source={{ file: card.png, format: 'png' }}
          columns={card.cols}
          rows={card.rows}
          alt={card.alt}
        />
      </Box>
    )
  })
}
