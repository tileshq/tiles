// claude plugin test adapters/claude
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

test('a folded run draws each finished chart, in the order the model made them', async ($, on) => {
  engineLine(on)
  const png = '/tmp/cache/cards/d4e5f6.png'
  const second = { ...CALL, output: `Tiles drew "Churn" as a line card (id d4e5f6, 40x10 cells).\n  PNG: ${png}` }
  const ui = await $.ui.mount({ ...GROUP, props: { ...GROUP.props, calls: [CALL, { ...CALL, tool: 'Read' }, second] } })
  const images = await ui.findAll({ type: 'Image' })
  expect(images.map(image => image.props.source.file)).toEqual([PNG, png])
  await ui.unmount()
})

test('the expanded row still draws the card in place of the result', async ($, on) => {
  on('ui.render', ($, e) => {
    const { Text } = $.ui.resolve(e)
    return <Text>raw result</Text>
  })
  const ui = await $.ui.mount({
    plugin: 'tiles',
    surface: 'terminal',
    component: 'ToolResult',
    requestId: 'row',
    props: { tool_use_id: 'row', tool: CALL.tool, output: OUTPUT, isErrored: false, isExpanded: true },
  })
  expect((await ui.find({ type: 'Image' }))?.props.source).toEqual({ file: PNG, format: 'png' })
  expect(await ui.find({ type: 'Text', text: /raw result/ })).toBeUndefined()
  await ui.unmount()
})
