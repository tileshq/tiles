'use client';

import { useLexicalComposerContext } from '@lexical/react/LexicalComposerContext';
import { useEffect, useRef, useState } from 'react';
import { $getSelection, $isRangeSelection, $isTextNode, COMMAND_PRIORITY_EDITOR, LexicalCommand, $createTextNode, $createParagraphNode, TextNode, $insertNodes, LexicalNode } from 'lexical';
import { $createHorizontalRuleNode } from '@lexical/react/LexicalHorizontalRuleNode';
import { useMcpContext } from '@/contexts/McpContext';
import { CSSProperties } from 'react';
import { createWasmExecutorFromBuffer, WasmExecutorResult, WasmExecutorOptions, WasmExecutor } from '../../lib/wasm-executor';
import { $createArtifactNode, $isArtifactNode, ArtifactContentType } from '../../nodes/ArtifactNode';
import { $setSelection } from 'lexical';

// Define a custom command for running MCP
export const RUN_MCP_COMMAND: LexicalCommand<void> = {
  type: 'RUN_MCP_COMMAND',
};

// Types for conversation
interface Message {
  role: 'user' | 'assistant' | 'system';
  content: any;
  type?: string;
}

interface ServletInfo {
  slug: string;
  contentAddress?: string;
  functionName?: string;
  config?: Record<string, any>;
  meta?: {
    schema?: {
      description?: string;
      tools?: {
        name: string;
        description: string;
        inputSchema: {
          type: string;
          properties: Record<string, any>;
          required?: string[];
        };
        parameters?: {
          properties: Record<string, any>;
        };
      }[];
      name?: string;
    };
  };
}

interface ServletTool {
  name: string;
  description: string;
  inputSchema: Record<string, any>;
  servletSlug: string;
}

interface ToolUseSubmessage {
  type: 'tool_use';
  id: string;
  name: string;
  input: any;
}

// --- Define Artifact Structure ---
interface ArtifactStructure {
    type: 'artifact';
    contentType: ArtifactContentType;
    content: string;
    metadata?: Record<string, any>; // Optional metadata
}

// Helper to check if an object is a valid artifact
function isValidArtifact(obj: any): obj is ArtifactStructure {
    // console.log('Validating artifact:', obj);
    const isValid = obj &&
           obj.type === 'artifact' &&
           typeof obj.contentType === 'string' &&
           typeof obj.content === 'string' &&
           ['application/vnd.ant.html', 'text/markdown', 'application/vnd.ant.mermaid'].includes(obj.contentType);
    
    // console.log('Artifact validation result:', isValid);
    if (!isValid) {
      // console.log('Validation failed because:');
      if (!obj) // console.log('- Object is null or undefined');
      if (obj && obj.type !== 'artifact') // console.log('- Type is not "artifact"');
      if (obj && typeof obj.contentType !== 'string') // console.log('- contentType is not a string');
      if (obj && typeof obj.content !== 'string') // console.log('- content is not a string');
      if (obj && typeof obj.contentType === 'string' && 
          !['application/vnd.ant.html', 'text/markdown', 'application/vnd.ant.mermaid'].includes(obj.contentType)) {
        // console.log('- contentType is not one of the allowed types');
      }
    }
    
    return isValid;
}

// Function to extract and parse artifact JSON from text
function extractArtifactFromText(text: string): ArtifactStructure | null {
  try {
    // First try to parse the entire text as JSON
    try {
      const parsed = JSON.parse(text);
      if (isValidArtifact(parsed)) {
        return parsed;
      }
    } catch (e) {
      // Not valid JSON, continue with extraction
    }
    
    // Try to extract JSON using regex
    const artifactMatch = text.match(/\{[\s\S]*"type":\s*"artifact"[\s\S]*\}/);
    if (artifactMatch) {
      const artifactJson = artifactMatch[0];
      // console.log('Extracted artifact JSON from text:', artifactJson.substring(0, 100) + '...');
      
      try {
        const parsed = JSON.parse(artifactJson);
        if (isValidArtifact(parsed)) {
          return parsed;
        }
      } catch (parseError) {
        // console.error('Error parsing extracted artifact JSON:', parseError);
        
        // If parsing failed, try to manually extract the content
        if (artifactJson.includes('"contentType":') && artifactJson.includes('"content":')) {
          try {
            // Extract contentType
            const contentTypeMatch = artifactJson.match(/"contentType":\s*"([^"]+)"/);
            const contentType = contentTypeMatch ? contentTypeMatch[1] : null;
            
            // Extract content - this is tricky because content might contain quotes and newlines
            // Find the start of the content
            const contentStartIndex = artifactJson.indexOf('"content":') + 10;
            if (contentStartIndex > 10) {
              // Find the first quote after the content start
              const firstQuoteIndex = artifactJson.indexOf('"', contentStartIndex);
              if (firstQuoteIndex > contentStartIndex) {
                // Find the matching end quote (this is tricky with nested quotes)
                let endQuoteIndex = -1;
                let inEscape = false;
                
                for (let i = firstQuoteIndex + 1; i < artifactJson.length; i++) {
                  if (artifactJson[i] === '\\' && !inEscape) {
                    inEscape = true;
                  } else if (artifactJson[i] === '"' && !inEscape) {
                    endQuoteIndex = i;
                    break;
                  } else {
                    inEscape = false;
                  }
                }
                
                if (endQuoteIndex > firstQuoteIndex) {
                  const content = artifactJson.substring(firstQuoteIndex + 1, endQuoteIndex);
                  
                  // Create a valid artifact object
                  if (contentType && ['application/vnd.ant.html', 'text/markdown', 'application/vnd.ant.mermaid'].includes(contentType)) {
                    return {
                      type: 'artifact',
                      contentType: contentType as ArtifactContentType,
                      content: content
                    };
                  }
                }
              }
            }
          } catch (e) {
            // console.error('Error manually extracting artifact content:', e);
          }
        }
      }
    }
    
    // If we couldn't extract using the above methods, try a more direct approach
    // Look for HTML content directly in the text
    if (text.includes('<html') || text.includes('<!DOCTYPE html')) {
      // console.log('Found HTML content directly in text');
      
      // Extract the HTML content
      const htmlMatch = text.match(/<html[\s\S]*<\/html>/i) || 
                        text.match(/<!DOCTYPE html[\s\S]*<\/html>/i) ||
                        text.match(/<body[\s\S]*<\/body>/i);
      
      if (htmlMatch) {
        const htmlContent = htmlMatch[0];
        // console.log('Extracted HTML content:', htmlContent.substring(0, 100) + '...');
        
        return {
          type: 'artifact',
          contentType: 'application/vnd.ant.html',
          content: htmlContent
        };
      }
    }
    
    return null;
  } catch (e) {
    // console.error('Error extracting artifact from text:', e);
    return null;
  }
}

// Styles for the plugin
const styles: Record<string, CSSProperties> = {
  button: {
    backgroundColor: 'transparent',
    color: 'inherit',
    border: 'none',
    borderRadius: '4px',
    padding: '8px',
    fontSize: '16px',
    cursor: 'pointer',
    display: 'flex',
    alignItems: 'center',
    justifyContent: 'center',
    transition: 'background-color 0.2s',
  },
  buttonDisabled: {
    opacity: 0.5,
    cursor: 'not-allowed',
  },
  error: {
    color: 'red',
    marginTop: '5px',
    fontSize: '12px',
  }
};

// Add ConfigPanel component before McpRunnerPlugin
function ConfigPanel({
  onClose,
  config,
  onConfigChange,
  runOnServer,
  onRunOnServerChange,
}: {
  onClose: () => void;
  config: Record<string, string>;
  onConfigChange: (newConfig: Record<string, string>) => void;
  runOnServer: boolean;
  onRunOnServerChange: (runOnServer: boolean) => void;
}): JSX.Element {
  const [keyValuePairs, setKeyValuePairs] = useState<Array<{key: string; value: string}>>(
    Object.entries(config).length > 0 
      ? Object.entries(config).map(([key, value]) => ({ key, value }))
      : [{ key: '', value: '' }]
  );

  // Log initial config
  useEffect(() => {
    //console.log('ConfigPanel mounted with initial config:', config);
  }, []);

  const handleAddPair = () => {
    //console.log('Adding new key-value pair');
    setKeyValuePairs([...keyValuePairs, { key: '', value: '' }]);
  };

  const handleRemovePair = (index: number) => {
    //console.log('Removing pair at index:', index);
    const newPairs = keyValuePairs.filter((_, i) => i !== index);
    setKeyValuePairs(newPairs);
    
    // Create a new config object from the remaining pairs
    const newConfig = newPairs.reduce((acc, { key, value }) => {
      if (key.trim() !== '' && value.trim() !== '') {
        // Remove any quotes from the key and value
        const cleanKey = key.replace(/^"|"$/g, '').trim();
        const cleanValue = value.replace(/^"|"$/g, '').trim();
        if (cleanKey && cleanValue) {
          acc[cleanKey] = cleanValue;
        }
      }
      return acc;
    }, {} as Record<string, string>);
    
    console.log('New config after removal:', newConfig);
    onConfigChange(newConfig);
  };

  const handlePairChange = (index: number, field: 'key' | 'value', newValue: string) => {
    //console.log(`Changing ${field} at index ${index} to:`, newValue);
    const newPairs = keyValuePairs.map((pair, i) => 
      i === index ? { ...pair, [field]: newValue } : pair
    );
    setKeyValuePairs(newPairs);
    
    // Create a new config object from the pairs, excluding empty pairs
    const newConfig = newPairs.reduce((acc, { key, value }) => {
      if (key !== '""' && value !== '""' && key.trim() !== '' && value.trim() !== '') {
        // Remove any quotes from the key and value
        const cleanKey = key.replace(/^"|"$/g, '').trim();
        const cleanValue = value.replace(/^"|"$/g, '').trim();
        if (cleanKey && cleanValue) {
          acc[cleanKey] = cleanValue;
        }
      }
      return acc;
    }, {} as Record<string, string>);
    
    console.log('New config being set:', newConfig);
    onConfigChange(newConfig);
  };

  // Log whenever keyValuePairs changes
  useEffect(() => {
    console.log('Current keyValuePairs:', keyValuePairs);
  }, [keyValuePairs]);

  return (
    <div className="config-panel">
      <div className="config-panel-header">
        <h3>Key configuration</h3>
        <button
          className="close-button"
          onClick={onClose}
          aria-label="Close config panel">
          ×
        </button>
      </div>
      <div className="config-panel-content">
        <div className="config-option">
          <label>
            <input
              type="checkbox"
              checked={runOnServer}
              onChange={(e) => {
                console.log('Server execution toggle changed to:', e.target.checked);
                onRunOnServerChange(e.target.checked);
              }}
            />
            Run on server (avoids CORS issues)
          </label>
        </div>
        <p>Add key-value pairs for your configuration.</p>
        {keyValuePairs.map((pair, index) => (
          <div key={index} className="config-pair">
            <input
              type="text"
              placeholder="Key"
              value={pair.key.replace(/^"|"$/g, '')} // Remove quotes for display
              onChange={(e) => handlePairChange(index, 'key', e.target.value)}
            />
            <input
              type="text"
              placeholder="Value"
              value={pair.value.replace(/^"|"$/g, '')} // Remove quotes for display
              onChange={(e) => handlePairChange(index, 'value', e.target.value)}
            />
            <button
              className="remove-pair"
              onClick={() => handleRemovePair(index)}
              aria-label="Remove pair">
              ×
            </button>
          </div>
        ))}
        <button
          className="add-pair"
          onClick={handleAddPair}>
          + Add Pair
        </button>
      </div>
    </div>
  );
}

export default function McpRunnerPlugin(): JSX.Element {
  const [editor] = useLexicalComposerContext();
  const { servlets, refreshServlets, isLoading, error, fetchWasmContent } = useMcpContext();
  const [wasmContent, setWasmContent] = useState<ArrayBuffer | null>(null);
  const [wasmError, setWasmError] = useState<string | null>(null);
  const [isProcessing, setIsProcessing] = useState(false);
  const [executionResult, setExecutionResult] = useState<WasmExecutorResult | null>(null);
  const [showConfig, setShowConfig] = useState(false);
  const [config, setConfig] = useState<Record<string, string>>({});
  const [runOnServer, setRunOnServer] = useState(false);
  
  // Debug: Track runOnServer state changes
  useEffect(() => {
    console.log('runOnServer state changed to:', runOnServer);
  }, [runOnServer]);
  
  // Create a ref to store the latest config
  const configRef = useRef(config);
  
  // Create a ref to store the latest runOnServer state
  const runOnServerRef = useRef(runOnServer);
  
  // Update configRef whenever config changes
  useEffect(() => {
    configRef.current = config;
    console.log('Config updated in McpRunnerPlugin:', config);
  }, [config]);
  
  // Update runOnServerRef whenever runOnServer changes
  useEffect(() => {
    runOnServerRef.current = runOnServer;
    console.log('runOnServer updated in ref:', runOnServer);
  }, [runOnServer]);
  
  // Configuration options for WasmExecutor - moved inside processCurrentSelection
  const getWasmExecutorOptions = (): WasmExecutorOptions => ({
    useWasi: true,
    allowedPaths: {
      '/tmp': '/tmp',
      '/data': '/data'
    },
    logLevel: 'debug',
    runInWorker: true,
    allowedHosts: ['*'],
    config: configRef.current // Use the current config from ref
  });
  
  // Create a ref to store the latest servlets data
  const servletsRef = useRef(servlets);
  
  // Update the ref whenever servlets changes
  useEffect(() => {
    servletsRef.current = servlets;
    //console.log('Servlets updated in ref:', servlets);
  }, [servlets]);

  // Insert text nodes after the current node
  const insertTextAfterNode = (targetNode: TextNode, text: string) => {
    editor.update(() => {
      const paragraph = $createParagraphNode();
      // Add indentation to the text
      const indentedText = '  ' + text;
      const textNode = $createTextNode(indentedText);
      paragraph.append(textNode);

      const targetParent = targetNode.getParentOrThrow();
      targetParent.insertAfter(paragraph);

      // Add horizontal rule after the text
      const horizontalRule = $createHorizontalRuleNode();
      paragraph.insertAfter(horizontalRule);
    });
  };

  // --- Insert Artifact Node ---
  const insertArtifactAfterNode = (targetNode: TextNode, artifact: ArtifactStructure) => {
      // console.log('Inserting artifact:', artifact);
      // console.log('Artifact content type:', artifact.contentType);
      // console.log('Artifact content length:', artifact.content.length);
      
      if (artifact.contentType === 'application/vnd.ant.html') {
        // console.log('HTML content preview:', artifact.content.substring(0, 100) + '...');
      }
      
      editor.update(() => {
          const artifactNode = $createArtifactNode(artifact.contentType, artifact.content);
          const paragraph = $createParagraphNode(); // Wrap artifact in a paragraph for block behavior
          paragraph.append(artifactNode);

          const targetParent = targetNode.getParentOrThrow();
          targetParent.insertAfter(paragraph); // Insert the paragraph containing the artifact

          // Optionally add a horizontal rule after the artifact paragraph
          const horizontalRule = $createHorizontalRuleNode();
          paragraph.insertAfter(horizontalRule);
      });
  };
  // --- End Insert Artifact Node ---

  // Function to execute WASM on server
  const executeWasmOnServer = async (
    contentAddress: string,
    functionName: string,
    input: string,
    config: Record<string, string>
  ) => {
    try {
      const response = await fetch('/api/wasm-execute', {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
        },
        body: JSON.stringify({
          contentAddress,
          functionName,
          input,
          config,
          executorOptions: {
            useWasi: true,
            allowedPaths: {
              '/tmp': '/tmp',
              '/data': '/data'
            },
            logLevel: 'debug',
            runInWorker: false
            // Note: allowedHosts is omitted because it requires runInWorker: true
          }
        }),
      });

      if (!response.ok) {
        const errorText = await response.text();
        try {
          const errorData = JSON.parse(errorText);
          throw new Error(errorData.error || 'Failed to execute WASM on server');
        } catch (parseError) {
          throw new Error(`Failed to execute WASM on server: ${errorText}`);
        }
      }

      const result = await response.json();
      if (result.error) {
        throw new Error(result.error);
      }

      return {
        output: result.output,
        error: undefined
      };
    } catch (error) {
      console.error('Error executing WASM on server:', error);
      return {
        output: '',
        error: error instanceof Error ? error.message : String(error)
      };
    }
  };

  // Make request to the Next.js API
  const callClaudeApi = async (messages: Message[], tools: any[]) => {
    try {
      const response = await fetch('/api/claude', {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
        },
        body: JSON.stringify({
          messages,
          tools,
        }),
      });

      if (!response.ok) {
        const errorText = await response.text();
        // console.error('Claude API error response:', errorText);
        try {
          const errorData = JSON.parse(errorText);
          throw new Error(errorData.error || 'Failed to connect to Claude API');
        } catch (parseError) {
          throw new Error(`Failed to connect to Claude API: ${errorText}`);
        }
      }

      const responseText = await response.text();
      //console.log('Raw Claude API response:', responseText);
      try {
        return JSON.parse(responseText);
      } catch (parseError) {
        // console.error('Failed to parse Claude API response:', parseError);
        throw new Error(`Invalid JSON response from Claude API: ${responseText}`);
      }
    } catch (error) {
      // console.error('Error calling Claude API:', error);
      throw error;
    }
  };

  // Function to process the current selection with agentic loop
  const processCurrentSelection = async () => {
    setIsProcessing(true);
    setExecutionResult(null);
    setWasmError(null);
    
    const selection = $getSelection();
    if (!$isRangeSelection(selection)) {
      setWasmError('Please select some text first');
      setIsProcessing(false);
      return;
    }
    
    // Get the selected text
    const userPrompt = selection.getTextContent().trim();

    // Clear/unselect the selection
    editor.update(() => {
      $setSelection(null);
    });
    
    if (!userPrompt) {
      setWasmError('Please select some text first');
      setIsProcessing(false);
      return;
    }
  
    // Find the last node in the selection to insert results after
    const anchorNode = selection.anchor.getNode();
    const focusNode = selection.focus.getNode();
    const lastNode = $isTextNode(focusNode) ? focusNode : 
                    ($isTextNode(anchorNode) ? anchorNode : null);
    
    if (!lastNode) {
      setWasmError('Selection must include text nodes');
      setIsProcessing(false);
      return;
    }
    
    // Find mcpserver node and subsequent nodes for input
    let mcpServerNode: TextNode | null = null;
    let hasFoundMcpNode = false;
    let processedPrompt = '';
    
    // Get all nodes in the selection
    const nodes = selection.getNodes();
    
    // Process each node in the selection
    for (const node of nodes) {
      if ($isTextNode(node) && node.getType() === 'mcpserver') {
        mcpServerNode = node;
        hasFoundMcpNode = true;
        continue;
      }
      
      if (hasFoundMcpNode && $isTextNode(node)) {
        processedPrompt += node.getTextContent() + ' ';
      }
    }
    
    if (!mcpServerNode) {
      setWasmError('No mcpserver node found in selection');
      setIsProcessing(false);
      return;
    }
    
    // Use either the processed prompt if we found nodes after mcpserver,
    // or fall back to the entire selection if not
    const finalPrompt = processedPrompt.trim() || userPrompt;
    
    if (!finalPrompt) {
      setWasmError('Please add your prompt after the mcpserver tag');
      setIsProcessing(false);
      return;
    }

    //console.log('Found mcpserver node:', {
    //  text: mcpServerNode.getTextContent(),
    //  key: mcpServerNode.getKey(),
    //  userPrompt: finalPrompt
    //});

    // Use the ref to access the latest servlets data
    const currentServlets = servletsRef.current;
    
    // Find the servlet matching the node's text content (assuming it's the slug)
    const servletSlug = mcpServerNode.getTextContent();
    const matchingServlet = currentServlets.find((servlet) => servlet.slug === servletSlug);

    if (!matchingServlet) {
      setWasmError(`Servlet with slug "${servletSlug}" not found`);
      setIsProcessing(false);
      return;
    }

    // Get the current WasmExecutor options with latest config
    const wasmExecutorOptions = getWasmExecutorOptions();
    console.log('Config being used for WASM executor:', wasmExecutorOptions.config);

    
    // Get content address from either meta.lastContentAddress or binding.contentAddress
    const contentAddress = matchingServlet.meta?.lastContentAddress || 
                          matchingServlet.binding?.contentAddress;
    
    if (!contentAddress) {
      setWasmError('No content address found for servlet');
      setIsProcessing(false);
      return;
    }
    
    // Insert initial message to show processing
    insertTextAfterNode(mcpServerNode, `Processing request: "${finalPrompt}"... (${runOnServerRef.current ? 'Server' : 'Local'} Execution)`);
    
    try {
      // Only fetch WASM content and create executor if running locally
      let executor: WasmExecutor | null = null;
      if (!runOnServerRef.current) {
        // Fetch WASM content
        const wasmBuffer = await fetchWasmContent(contentAddress);
        setWasmContent(wasmBuffer);
        
        // Create the WASM executor with current options
        console.log('Creating WASM executor with options:', wasmExecutorOptions);
        executor = await createWasmExecutorFromBuffer(wasmBuffer, wasmExecutorOptions);
        
        // Add a small delay to allow initialization to complete
        await new Promise(resolve => setTimeout(resolve, 250));
      }
      
      
          // Extract all available tools from the servlet schema
    const availableTools = matchingServlet.meta?.schema?.tools || [];
    
    if (availableTools.length === 0) {
      setWasmError(`No tools found in servlet "${servletSlug}"`);
      setIsProcessing(false);
      return;
    }
    
    // Format all tools for Claude API
    const claudeTools = availableTools.map(tool => ({
      name: tool.name,
      description: tool.description,
      input_schema: {
        type: "object",
        properties: tool.inputSchema?.properties || {},
        required: tool.inputSchema?.required || []
      }
    }));
      
      // --- Artifact System Prompt ---
      // Prepare a system message to instruct Claude about artifact formats
      const artifactSystemMessage: Message = {
        role: 'system',
        content: `When generating visual content such as diagrams, charts, HTML, or formatted content, please respond with a JSON object that follows this structure:
      {
        "type": "artifact",
        "contentType": "application/vnd.ant.html" | "text/markdown" | "application/vnd.ant.mermaid",
        "content": "your content here"
      }
      
      Content type selection:
      - Diagrams, flowcharts, mind maps → "application/vnd.ant.mermaid"
      - Interactive web content, styled text → "application/vnd.ant.html"
      - Documentation, notes, plain formatted text → "text/markdown"
      - When unclear, prefer Mermaid for diagrams and HTML for everything else
      
      When asked to create any diagram, flowchart, or visual representation:
      - Default to Mermaid syntax unless specifically asked for another format
      - Use "application/vnd.ant.mermaid" as the contentType
      - Include proper Mermaid syntax with correct node definitions and connections
      
      Content escaping rules:
      - For JSON strings: escape backslashes (\\\\), quotes (\\"), newlines (\\n), tabs (\\t)
      - For HTML: escape < as &lt;, > as &gt;, & as &amp;
      - For Mermaid: use double backslashes for line breaks (\\\\n)
      - Always validate that the JSON structure remains valid after escaping
      
      Common mistakes to avoid:
      - Never mix artifact types (e.g., HTML inside Mermaid)
      - Always close all HTML tags properly
      - Ensure Mermaid syntax follows proper node naming (no spaces in node IDs)
      - Test that all escape sequences maintain valid JSON
      
      For code artifacts:
      - Use HTML with <pre><code> tags for syntax highlighting
      - Include proper indentation using spaces (not tabs)
      - Escape all HTML entities within code blocks
      - Consider adding inline CSS for basic code styling
      
      Examples:
      
      Mermaid diagram:
      {
        "type": "artifact",
        "contentType": "application/vnd.ant.mermaid",
        "content": "graph TD\\n    A[Start Process]-->B{Decision Point}\\n    B-->|Yes| C[Action 1]\\n    B-->|No| D[Action 2]\\n    C-->E[End]\\n    D-->E"
      }
      
      HTML with special characters:
      {
        "type": "artifact",
        "contentType": "application/vnd.ant.html",
        "content": "<div style=\\"font-family: Arial, sans-serif; padding: 20px;\\">\\n    <h1>Title with &quot;quotes&quot;</h1>\\n    <p>First line<br>\\n    Second line with &amp; symbol</p>\\n    <ul>\\n        <li>Item 1</li>\\n        <li>Item 2</li>\\n    </ul>\\n</div>"
      }
      
      Markdown:
      {
        "type": "artifact",
        "contentType": "text/markdown",
        "content": "# Heading\\n\\n## Subheading\\n\\n- Bullet point 1\\n- Bullet point 2\\n\\n**Bold text** and *italic text*"
      }
      
      Before returning the JSON:
      1. Verify the JSON structure is valid
      2. Check that content string has proper escaping
      3. Ensure contentType matches the actual content format
      4. Confirm no mixing of different content types
      
      IMPORTANT: Return ONLY the JSON object. Do not include:
      - Markdown code blocks (\`\`\`json)
      - Explanatory text before or after
      - Additional formatting or wrapper elements
      - Any text outside the JSON structure
      
      The response must start with { and end with }`
      };

      // Start the conversation with the initial message
      let messages: Message[] = [
        artifactSystemMessage, // Add the system message about artifacts first
        { role: 'user', content: finalPrompt }
      ];
      // --- End Artifact System Prompt ---
      
      // Keep track of conversation history for display
      let conversationHistory: Message[] = [{
        role: 'user',
        content: finalPrompt
      }];
      
      let response;
      
      // Insert user message to conversation display
      insertTextAfterNode(mcpServerNode, `tile: ${finalPrompt}`);
      
      // Agentic loop - continue running until we get a final message
      do {
        // Send the current state of the conversation to Claude via API
        try {
          //console.log('Sending messages to Claude:', messages, claudeTools);
          
          // Call Claude API with messages that include our system prompt
          response = await callClaudeApi(messages, claudeTools);
          // console.log('Claude API response structure:', {
          //   role: response.role,
          //   contentLength: response.content.length,
          //   stopReason: response.stop_reason
          // });
        } catch (error) {
          // console.error('Error calling Claude API:', error);
          setWasmError(`Error calling Claude API: ${error instanceof Error ? error.message : String(error)}`);
          insertTextAfterNode(mcpServerNode, `Error calling Claude API: ${error instanceof Error ? error.message : String(error)}`);
          break;
        }
        
        // Add Claude's response to messages and conversation history
        messages.push({
          role: response.role,
          content: response.content,
        });
        
        conversationHistory.push({
          role: response.role,
          content: response.content,
        });
        
        // --- Process and Display Claude's Response ---
        let claudeNonArtifactResponse = '';

        // First check if the entire response is an artifact
        try {
          const responseText = response.content
            .filter((part: any) => part.type === 'text')
            .map((part: any) => part.text)
            .join('')
            .trim();

          // console.log('Response text for artifact check:', responseText.substring(0, 100) + '...');
          
          if (responseText.includes('"type": "artifact"')) {
            // console.log('Found potential artifact in response');
            
            // Try to extract and parse the artifact
            const artifact = extractArtifactFromText(responseText);
            if (artifact) {
              // console.log('Successfully extracted artifact:', {
              //   type: artifact.type,
              //   contentType: artifact.contentType,
              //   contentLength: artifact.content.length
              // });
              
              // Insert the artifact node
              insertArtifactAfterNode(mcpServerNode, artifact);
              continue; // Skip to next iteration since this was a pure artifact response
            }
          }
        } catch (e) {
          // Not a valid artifact JSON, continue with normal processing
          // console.warn('Response was not a valid artifact:', e);
        }

        // Process each part of the response
        for (const part of response.content) {
          if (part.type === 'text') {
            claudeNonArtifactResponse += part.text;
          } else if (part.type === 'tool_use') {
            // Handle tool use as before (potentially insert pending text first)
            if (claudeNonArtifactResponse) {
              insertTextAfterNode(mcpServerNode, `Claude: ${claudeNonArtifactResponse}`);
              claudeNonArtifactResponse = ''; // Reset accumulator
            }
            // Tool use handling continues below...
          }
        }

        // Insert any remaining non-artifact text at the end
        if (claudeNonArtifactResponse) {
          // Check if the non-artifact text contains an artifact JSON
          if (claudeNonArtifactResponse.includes('"type": "artifact"')) {
            // Try to extract and parse the artifact
            const artifact = extractArtifactFromText(claudeNonArtifactResponse);
            if (artifact) {
              // console.log('Successfully extracted artifact from text:', {
              //   type: artifact.type,
              //   contentType: artifact.contentType,
              //   contentLength: artifact.content.length
              // });
              
              // Insert the artifact node
              insertArtifactAfterNode(mcpServerNode, artifact);
              
              // Remove the artifact JSON from the text to display
              // This is approximate since we don't know the exact position
              claudeNonArtifactResponse = claudeNonArtifactResponse.replace(
                /\{[\s\S]*"type":\s*"artifact"[\s\S]*\}/, 
                ''
              );
            }
          }
          
          // Only insert the text if it's not empty after processing
          if (claudeNonArtifactResponse.trim()) {
            insertTextAfterNode(mcpServerNode, `Claude: ${claudeNonArtifactResponse}`);
          }
        }
        // --- End Process and Display Claude's Response ---
        
        // Check if there are any tool use requests
        const newMessage: Message = { role: 'user', content: [] };
        let toolUseCount = 0;
        
        for (const submessage of response.content) {
          if (submessage.type !== 'tool_use') {
            continue;
          }
          
          toolUseCount++;
          const { id, input, name } = submessage as unknown as ToolUseSubmessage;
          
          try {
            // Display tool call
            insertTextAfterNode(mcpServerNode, `Tool call: ${name}\nInput: ${JSON.stringify(input, null, 2)}`);
            
            // Prepare the input for the servlet
            const servletInput = JSON.stringify({
              params: {
                name: name,
                arguments: input
              }
            });
            
            //console.log(`Executing tool ${name} with input:`, servletInput);
            
            // Execute the servlet using either server or local execution
            const useServerExecution = runOnServerRef.current;
            console.log('Tool execution mode:', useServerExecution ? 'Server' : 'Local');
            let executionResult;
            if (useServerExecution) {
              // Execute on server
              console.log('Executing on server with contentAddress:', contentAddress);
              insertTextAfterNode(mcpServerNode, `🌐 Executing ${name} on server...`);
              executionResult = await executeWasmOnServer(
                contentAddress,
                'call',
                servletInput,
                configRef.current
              );
            } else {
              // Execute locally using the plugin
              console.log('Executing locally');
              insertTextAfterNode(mcpServerNode, `💻 Executing ${name} locally...`);
              if (!executor) {
                throw new Error('Local executor not initialized');
              }
              executionResult = await executor.execute('call', servletInput);
            }
            setExecutionResult(executionResult);
            
            if (executionResult.error) {
              throw new Error(executionResult.error);
            }
            
            // Get the result
            const resultText = executionResult.output;
            let parsedResult;
            
            try {
              // Try to parse as JSON if it looks like JSON
              if (resultText.startsWith('{') && resultText.endsWith('}')) {
                parsedResult = JSON.parse(resultText);
              } else {
                parsedResult = resultText;
              }
            } catch (e) {
              parsedResult = resultText;
            }
            
            // Display tool result
            insertTextAfterNode(
              mcpServerNode, 
              `Tool result: ${typeof parsedResult === 'object' ? JSON.stringify(parsedResult, null, 2) : parsedResult}`
            );
            
            // Add the tool result to the message
            newMessage.content.push({
              type: 'tool_result',
              tool_use_id: id,
              content: typeof parsedResult === 'object' ? JSON.stringify(parsedResult) : String(parsedResult)
            });
            
            // Track for history display
            conversationHistory.push({
              role: 'user',
              type: 'tool_results',
              content: [{
                toolName: name,
                input,
                result: parsedResult
              }]
            });
          } catch (error) {
            // console.error(`Error executing tool ${name}:`, error);
            
            const errorMessage = error instanceof Error ? error.message : String(error);
            
            // Display error
            insertTextAfterNode(mcpServerNode, `Error: ${errorMessage}`);
            
            // Add the error as a tool result
            newMessage.content.push({
              type: 'tool_result',
              tool_use_id: id,
              content: `Error: ${errorMessage}`,
              is_error: true
            });
            
            // Track for history display
            conversationHistory.push({
              role: 'user',
              type: 'tool_results',
              content: [{
                toolName: name,
                input,
                error: errorMessage
              }]
            });
          }
        }
        
        // If Claude is doing tool use, add the result as a user message and continue
        if (response.stop_reason === 'tool_use') {
          messages.push(newMessage);
          continue;
        }
        
        // If there was tool use but Claude is now done its turn, add the results and continue
        if (response.stop_reason === 'end_turn' && toolUseCount > 0) {
          messages.push(newMessage);
          continue;
        }
        
        // Otherwise, we're done
        break;
        
      } while (true);
      
      //console.log(`Conversation complete.`);
      
      // Clean up the executor if it was created
      if (executor) {
        await executor.free();
      }
      
      // Insert final completion message
      insertTextAfterNode(mcpServerNode, "✅ Processing complete");
      
    } catch (err) {
      // console.error('Error in agentic loop:', err);
      setWasmError(err instanceof Error ? err.message : String(err));
      insertTextAfterNode(mcpServerNode, `Error: ${err instanceof Error ? err.message : String(err)}`);
    } finally {
      setIsProcessing(false);
    }
  };

  useEffect(() => {
    // Register the custom command
    const unregisterCommand = editor.registerCommand(
      RUN_MCP_COMMAND,
      () => {
        processCurrentSelection();
        return true;
      },
      COMMAND_PRIORITY_EDITOR
    );

    // Register keyboard shortcut (Ctrl+Enter or Cmd+Enter)
    const unregisterKeyDown = editor.registerRootListener((rootElement, prevRootElement) => {
      if (rootElement !== null) {
        rootElement.addEventListener('keydown', (event) => {
          // Check for Ctrl+Enter or Cmd+Enter
          if ((event.ctrlKey || event.metaKey) && event.key === 'Enter') {
            event.preventDefault();
            event.stopPropagation(); // Also stop propagation to prevent other handlers
            editor.dispatchCommand(RUN_MCP_COMMAND, undefined);
          }
        }, true); // Use capture phase to handle the event before other handlers
      }
    });

    return () => {
      unregisterCommand();
      unregisterKeyDown();
    };
  }, [editor]);

  // Handle the button click
  const handleRunMcp = () => {
    editor.dispatchCommand(RUN_MCP_COMMAND, undefined);
  };

  // This component will be rendered in the toolbar
  return (
    <div>
      <button 
        onClick={() => setShowConfig(!showConfig)}
        style={{
          ...styles.button,
          position: 'fixed',
          top: '175px', // Position below the servlets button (125px + 40px height + 10px gap)
          right: '20px',
          zIndex: 100,
          backgroundColor: '#ffffff',
          boxShadow: '0px 1px 5px rgba(0, 0, 0, 0.3)',
          width: '40px',
          height: '40px',
          borderRadius: '20px',
          padding: 0,
          display: 'flex',
          alignItems: 'center',
          justifyContent: 'center'
        }}
        title="Configure"
        className="toolbar-item"
      >
        <i className="settings" />
      </button>
      <button 
        onClick={handleRunMcp}
        disabled={isProcessing}
        style={isProcessing ? {...styles.button, ...styles.buttonDisabled} : styles.button}
        title="Run Tiles (Ctrl+Enter)"
        className="toolbar-item"
      >
      Run Tiles  <img src="/icon.png" alt="Run Icon" style={{height: '1em', width: '1em', verticalAlign: 'middle', marginRight: '0.45em', marginLeft: '0.45em'}} />
      </button>
      {wasmError && <div style={styles.error}>{wasmError}</div>}
      {showConfig && (
        <ConfigPanel
          onClose={() => setShowConfig(false)}
          config={config}
          onConfigChange={setConfig}
          runOnServer={runOnServer}
          onRunOnServerChange={setRunOnServer}
        />
      )}
    </div>
  );
}