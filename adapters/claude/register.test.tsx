// claude plugin test adapters/claude
//
// That command also tries card.test.ts, a node test the kit cannot load, and
// exits 1 for it, so read this file's results.
//
// Each test mounts a component through the mod on the engine's own kit; the
// `ui.render` hook on `on` stands for what Claude Code draws beneath the mod.
import { expect, test } from 'claude-code/testing'

const PNG = '/tmp/cache/cards/a1b2c3.png'
const OUTPUT = `Tiles drew "Signups" as a column card (id a1b2c3, 52x12 cells).\n  PNG: ${PNG}`
const CALL = {
  tool: 'mcp__tiles__tiles_chart',
  input: {},
  isRunning: false,
  isErrored: false,
  isInterrupted: false,
  output: OUTPUT,
}
const GROUP = {
  plugin: 'tiles',
  surface: 'terminal',
  component: 'ToolGroup',
  requestId: 'group',
  props: { calls: [CALL], isActive: false, isExpanded: false },
} as const

const engineLine = on =>
  on('ui.render', ($, e) => {
    const { Text } = $.ui.resolve(e)
    return <Text>Called tiles</Text>
  })

test('the default view draws the card under the folded line', async ($, on) => {
  engineLine(on)
  const ui = await $.ui.mount(GROUP)
  expect((await ui.find({ type: 'Text', text: /Called tiles/ }))?.text).toBe('Called tiles')
  expect((await ui.find({ type: 'Image' }))?.props.source).toEqual({ file: PNG, format: 'png' })
  await ui.unmount()
})

test('an expanded group is left to its rows', async ($, on) => {
  engineLine(on)
  const ui = await $.ui.mount({ ...GROUP, props: { ...GROUP.props, isExpanded: true } })
  expect(await ui.find({ type: 'Image' })).toBeUndefined()
  await ui.unmount()
})

test('a group with no finished chart is left to Claude Code', async ($, on) => {
  engineLine(on)
  const calls = [{ ...CALL, tool: 'Read' }, { ...CALL, isErrored: true }, { ...CALL, output: undefined, isRunning: true }]
  const ui = await $.ui.mount({ ...GROUP, props: { ...GROUP.props, calls } })
  expect(await ui.find({ type: 'Image' })).toBeUndefined()
  expect((await ui.find({ type: 'Text', text: /Called tiles/ }))?.text).toBe('Called tiles')
  await ui.unmount()
})
