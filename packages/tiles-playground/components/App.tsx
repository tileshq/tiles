'use client';

import {$createLinkNode} from '@lexical/link';
import {$createListItemNode, $createListNode} from '@lexical/list';
import {LexicalComposer} from '@lexical/react/LexicalComposer';
import {$createHeadingNode, $createQuoteNode} from '@lexical/rich-text';
import {
  $createParagraphNode,
  $createTextNode,
  $getRoot,
  $isTextNode,
  DOMConversionMap,
  TextNode,
} from 'lexical';

import {isDevPlayground} from '@/utils/appSettings';
import {useSettings} from '@/context/SettingsContext';
import {SharedHistoryContext} from '@/context/SharedHistoryContext';
import {TableContext} from '@/plugins/TablePlugin';
import {ToolbarContext} from '@/context/ToolbarContext';
import DocumentHead from '@/components/DocumentHead';
import Editor from '@/components/Editor';
import PlaygroundNodes from '@/nodes/PlaygroundNodes';
//import DocsPlugin from '@/plugins/DocsPlugin';
//import PasteLogPlugin from '@/plugins/PasteLogPlugin';
//import TestRecorderPlugin from '@/plugins/TestRecorderPlugin';
import {parseAllowedFontSize} from '@/plugins/ToolbarPlugin/fontSize';
import TypingPerfPlugin from '@/plugins/TypingPerfPlugin';
import Settings from '@/components/Settings';
import PlaygroundEditorTheme from '@/themes/PlaygroundEditorTheme';
import {parseAllowedColor} from '@/ui/ColorPicker';

function $prepopulatedRichText() {
  const root = $getRoot();
  if (root.getFirstChild() === null) {
    const heading = $createHeadingNode('h1');
    heading.append($createTextNode('Welcome to the playground'));
    root.append(heading);
    const quote = $createQuoteNode();
    quote.append(
      $createTextNode(
        `In case you were wondering what the black box at the bottom is – it's the debug view, showing the current state of the editor. ` +
          `You can disable it by pressing on the settings control in the bottom-left of your screen and toggling the debug view setting.`,
      ),
    );
    root.append(quote);
    const paragraph = $createParagraphNode();
    paragraph.append(
      $createTextNode('The playground is a demo environment built with '),
      $createTextNode('@lexical/react').toggleFormat('code'),
      $createTextNode('.'),
      $createTextNode(' Try typing in '),
      $createTextNode('some text').toggleFormat('bold'),
      $createTextNode(' with '),
      $createTextNode('different').toggleFormat('italic'),
      $createTextNode(' formats.'),
    );
    root.append(paragraph);
    const paragraph2 = $createParagraphNode();
    paragraph2.append(
      $createTextNode(
        'Make sure to check out the various plugins in the toolbar. You can also use #hashtags or @-mentions too!',
      ),
    );
    root.append(paragraph2);
    const paragraph3 = $createParagraphNode();
    paragraph3.append(
      $createTextNode(`If you'd like to find out more about Lexical, you can:`),
    );
    root.append(paragraph3);
    const list = $createListNode('bullet');
    list.append(
      $createListItemNode().append(
        $createTextNode(`Visit the `),
        $createLinkNode('https://lexical.dev/').append(
          $createTextNode('Lexical website'),
        ),
        $createTextNode(` for documentation and more information.`),
      ),
      $createListItemNode().append(
        $createTextNode(`Check out the code on our `),
        $createLinkNode('https://github.com/facebook/lexical').append(
          $createTextNode('GitHub repository'),
        ),
        $createTextNode(`.`),
      ),
      $createListItemNode().append(
        $createTextNode(`Playground code can be found `),
        $createLinkNode(
          'https://github.com/facebook/lexical/tree/main/packages/lexical-playground',
        ).append($createTextNode('here')),
        $createTextNode(`.`),
      ),
      $createListItemNode().append(
        $createTextNode(`Join our `),
        $createLinkNode('https://discord.com/invite/KmG4wQnnD9').append(
          $createTextNode('Discord Server'),
        ),
        $createTextNode(` and chat with the team.`),
      ),
    );
    root.append(list);
    const paragraph4 = $createParagraphNode();
    paragraph4.append(
      $createTextNode(
        `Lastly, we're constantly adding cool new features to this playground. So make sure you check back here when you next get a chance :).`,
      ),
    );
    root.append(paragraph4);
  }
}

function getExtraStyles(element: HTMLElement): string {
  // Parse styles from pasted input, but only if they match exactly the
  // sort of styles that would be produced by exportDOM
  let extraStyles = '';
  const fontSize = parseAllowedFontSize(element.style.fontSize);
  const backgroundColor = parseAllowedColor(element.style.backgroundColor);
  const color = parseAllowedColor(element.style.color);
  if (fontSize !== '' && fontSize !== '15px') {
    extraStyles += `font-size: ${fontSize};`;
  }
  if (backgroundColor !== '' && backgroundColor !== 'rgb(255, 255, 255)') {
    extraStyles += `background-color: ${backgroundColor};`;
  }
  if (color !== '' && color !== 'rgb(0, 0, 0)') {
    extraStyles += `color: ${color};`;
  }
  return extraStyles;
}

function buildImportMap(): DOMConversionMap {
  const importMap: DOMConversionMap = {};

  // Wrap all TextNode importers with a function that also imports
  // the custom styles implemented by the playground
  for (const [tag, fn] of Object.entries(TextNode.importDOM() || {})) {
    importMap[tag] = (importNode) => {
      const importer = fn(importNode);
      if (!importer) {
        return null;
      }
      return {
        ...importer,
        conversion: (element) => {
          const output = importer.conversion(element);
          if (
            output === null ||
            output.forChild === undefined ||
            output.after !== undefined ||
            output.node !== null
          ) {
            return output;
          }
          const extraStyles = getExtraStyles(element);
          if (extraStyles) {
            const {forChild} = output;
            return {
              ...output,
              forChild: (child, parent) => {
                const textNode = forChild(child, parent);
                if ($isTextNode(textNode)) {
                  textNode.setStyle(textNode.getStyle() + extraStyles);
                }
                return textNode;
              },
            };
          }
          return output;
        },
      };
    };
  }

  return importMap;
}

function App(): JSX.Element {
  const {
    settings: {isCollab, emptyEditor, measureTypingPerf},
  } = useSettings();

  const initialConfig = {
    editorState: isCollab
      ? null
      : emptyEditor
      ? undefined
      : $prepopulatedRichText,
    html: {import: buildImportMap()},
    namespace: 'Playground',
    nodes: [...PlaygroundNodes],
    onError: (error: Error) => {
      throw error;
    },
    theme: PlaygroundEditorTheme,
  };

  return (
    <LexicalComposer initialConfig={initialConfig}>
      <SharedHistoryContext>
        <TableContext>
          <ToolbarContext>
            <DocumentHead />
            <header>
              <a href="https://tiles.run" target="_blank" rel="noreferrer">
                <img
                  src="/icon.png"
                  alt="Lexical Logo"
                  style={{height: 'auto', width: '32px'}}
                />
              </a>
            </header>
            <div className="editor-shell">
              <Editor />
            </div>
            <Settings />
            {/*isDevPlayground ? <DocsPlugin /> : null */}
            {/*isDevPlayground ? <PasteLogPlugin /> : null*/}
            {/*isDevPlayground ? <TestRecorderPlugin /> : null*/}

            {measureTypingPerf ? <TypingPerfPlugin /> : null}
          </ToolbarContext>
        </TableContext>
      </SharedHistoryContext>
      <footer
        style={{
          borderTop: '1px solid #eee',
          color: '#666',
          fontSize: '14px',
          marginTop: '20px',
          padding: '20px',
          textAlign: 'center',
        }}>
        <p>A new kind of notebook for making personal software.</p>
        <p>Technically it's a local-first, multiplayer enabled MCP client with notebook interface.</p>
        <p>View <a
          href="https://www.tiles.run/shared/Aa2sK9GLwHFkCZOPevge-"
          target="_blank"
          rel="noreferrer"
          style={{color: '#007bff', textDecoration: 'none'}}
        >
          Live Changelog
        </a>.</p>
      </footer>
    </LexicalComposer>
  );
}

export default App;