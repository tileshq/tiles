'use client';

import React, { useState, useEffect, useRef, useCallback } from 'react';
import mermaid from 'mermaid';
import ReactMarkdown from 'react-markdown';
import type { NodeKey } from 'lexical';
import type { ArtifactContentType } from '../nodes/ArtifactNode';
import DOMPurify from 'dompurify';
import './ArtifactRenderer.css';

// Function to check if content is valid HTML
const isValidHTML = (content: string): boolean => {
  // Basic check for HTML structure
  return content.trim().toLowerCase().startsWith('<html') || 
         content.trim().toLowerCase().startsWith('<!doctype html') ||
         content.includes('<body') || 
         content.includes('<div') ||
         content.includes('<p') ||
         content.includes('<br') ||
         content.includes('<span');
};

// Function to fix common HTML content issues
const fixHTMLContent = (content: string): string => {
  // Replace escaped newlines with actual newlines
  let fixed = content.replace(/\\n/g, '\n');
  
  // Replace escaped quotes with actual quotes
  fixed = fixed.replace(/\\"/g, '"');
  
  // Replace escaped backslashes with actual backslashes
  fixed = fixed.replace(/\\\\/g, '\\');
  
  // Ensure proper HTML structure if it's just a fragment
  if (!fixed.trim().toLowerCase().startsWith('<html') && 
      !fixed.trim().toLowerCase().startsWith('<!doctype html')) {
    // Wrap in a basic HTML structure if it's just a fragment
    fixed = `<div class="html-fragment">${fixed}</div>`;
  }
  
  return fixed;
};

interface ArtifactRendererProps {
  contentType: ArtifactContentType;
  content: string;
  nodeKey: NodeKey;
}

// Content type configuration
const CONTENT_TYPE_CONFIG: Record<ArtifactContentType, { label: string; icon: string; color: string }> = {
  'application/vnd.ant.html': { label: 'HTML', icon: '🌐', color: '#e34c26' },
  'text/markdown': { label: 'Markdown', icon: '📝', color: '#083fa1' },
  'application/vnd.ant.mermaid': { label: 'Mermaid', icon: '📊', color: '#ff3670' },
};

// Initialize Mermaid
mermaid.initialize({
  startOnLoad: false, // We'll trigger rendering manually
  theme: 'default', // Or 'dark', 'neutral', etc.
  // Consider securityLevel: 'strict' or 'sandbox' if needed
});

const ArtifactRenderer: React.FC<ArtifactRendererProps> = ({ contentType, content, nodeKey }) => {
  const [showCode, setShowCode] = useState(false);
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);
  const mermaidRef = useRef<HTMLDivElement>(null);
  const iframeRef = useRef<HTMLIFrameElement>(null);
  const [mermaidRendered, setMermaidRendered] = useState(false);
  const [iframeLoaded, setIframeLoaded] = useState(false);
  const [uniqueMermaidId] = useState(`mermaid-${nodeKey}-${Date.now()}`);
  const [iframeHeight, setIframeHeight] = useState(400);

  const typeConfig = CONTENT_TYPE_CONFIG[contentType];

  const handleCopyToClipboard = useCallback(async () => {
    try {
      await navigator.clipboard.writeText(content);
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    } catch (err) {
      console.error('Failed to copy to clipboard:', err);
    }
  }, [content]);

  const renderMermaid = useCallback(async () => {
    if (mermaidRef.current && !mermaidRendered) {
      setIsLoading(true);
      setError(null);
      try {
        // Ensure the container is clean before rendering
        mermaidRef.current.innerHTML = ''; 
        const { svg } = await mermaid.render(uniqueMermaidId, content);
        if (mermaidRef.current) { // Check ref again inside async
          mermaidRef.current.innerHTML = svg;
          setMermaidRendered(true);
        }
      } catch (error) {
        const errorMessage = error instanceof Error ? error.message : String(error);
        setError(`Failed to render Mermaid diagram: ${errorMessage}`);
        if (mermaidRef.current) {
          mermaidRef.current.innerHTML = '';
        }
      } finally {
        setIsLoading(false);
      }
    }
  }, [content, uniqueMermaidId, mermaidRendered]);

  // Auto-adjust iframe height based on content
  const adjustIframeHeight = useCallback(() => {
    if (iframeRef.current) {
      try {
        const iframeDocument = iframeRef.current.contentDocument;
        if (iframeDocument) {
          const height = Math.min(
            Math.max(iframeDocument.documentElement.scrollHeight + 20, 200),
            800
          );
          setIframeHeight(height);
        }
      } catch (err) {
        // Cross-origin restrictions might prevent access
        console.warn('Could not adjust iframe height:', err);
      }
    }
  }, []);

  const handleIframeLoad = useCallback(() => {
    setIframeLoaded(true);
    adjustIframeHeight();
  }, [adjustIframeHeight]);

  useEffect(() => {
    if (contentType === 'application/vnd.ant.mermaid' && !showCode) {
      renderMermaid();
    }
    // Reset rendered state if content changes (e.g., node edited)
    return () => setMermaidRendered(false);
  }, [contentType, content, showCode, renderMermaid]);

  useEffect(() => {
    // Reset iframe loaded state when content changes
    if (contentType === 'application/vnd.ant.html') {
      setIframeLoaded(false);
    }
  }, [contentType, content]);

  const toggleView = () => setShowCode(!showCode);

  const renderErrorState = () => (
    <div className="artifact-error">
      <div className="artifact-error-icon">⚠️</div>
      <div className="artifact-error-content">
        <h4>Error Rendering Content</h4>
        <p>{error}</p>
        <button 
          className="artifact-retry-button"
          onClick={() => {
            setError(null);
            if (contentType === 'application/vnd.ant.mermaid') {
              setMermaidRendered(false);
              renderMermaid();
            }
          }}
        >
          Try Again
        </button>
      </div>
    </div>
  );

  const renderLoadingState = () => (
    <div className="artifact-loading">
      <div className="artifact-loading-spinner"></div>
      <p>Rendering {typeConfig.label}...</p>
    </div>
  );

  const renderContent = () => {
    if (error) {
      return renderErrorState();
    }

    if (isLoading) {
      return renderLoadingState();
    }

    if (showCode) {
      return (
        <div className="artifact-code-container">
          <pre className="artifact-code-block">
            <code>{content}</code>
          </pre>
        </div>
      );
    }

    switch (contentType) {
      case 'application/vnd.ant.html':
        const fixedContent = fixHTMLContent(content);
        
        if (!isValidHTML(fixedContent)) {
          return (
            <div className="artifact-html-container">
              <div className="artifact-invalid-html">
                <p>⚠️ Invalid HTML content. Showing as code:</p>
                <pre className="artifact-code-block"><code>{fixedContent}</code></pre>
              </div>
            </div>
          );
        }
        
        const sanitizedHTML = DOMPurify.sanitize(fixedContent, {
          ADD_TAGS: ['style', 'script'],
          ADD_ATTR: ['onclick', 'onload', 'onerror'],
          FORBID_TAGS: [],
          FORBID_ATTR: []
        });
        
        return (
          <div className="artifact-html-container">
            {!iframeLoaded && (
              <div className="artifact-loading">
                <div className="artifact-loading-spinner"></div>
                <p>Loading HTML content...</p>
              </div>
            )}
            <iframe
              ref={iframeRef}
              srcDoc={sanitizedHTML}
              className="artifact-iframe"
              style={{ 
                height: `${iframeHeight}px`,
                opacity: iframeLoaded ? 1 : 0,
                transition: 'opacity 0.3s ease'
              }}
              sandbox="allow-same-origin allow-scripts"
              title="HTML Content"
              onLoad={handleIframeLoad}
            />
          </div>
        );

      case 'text/markdown':
        return (
          <div className="artifact-markdown-container">
            <ReactMarkdown>{content}</ReactMarkdown>
          </div>
        );

      case 'application/vnd.ant.mermaid':
        return (
          <div className="artifact-mermaid-container">
            <div ref={mermaidRef} className="artifact-mermaid-content" />
          </div>
        );

      default:
        return (
          <div className="artifact-unsupported">
            <p>⚠️ Unsupported artifact type: <code>{contentType}</code></p>
          </div>
        );
    }
  };

  return (
    <div className="artifact-container">
      {/* Header with type badge and controls */}
      <div className="artifact-header">
        <div className="artifact-type-badge" style={{ backgroundColor: typeConfig.color }}>
          <span className="artifact-type-icon">{typeConfig.icon}</span>
          <span className="artifact-type-label">{typeConfig.label}</span>
        </div>
        
        <div className="artifact-controls">
          <button
            className="artifact-control-button"
            onClick={handleCopyToClipboard}
            title="Copy to clipboard"
            aria-label="Copy content to clipboard"
          >
            {copied ? '✓' : '📋'}
          </button>
          
          <button
            className={`artifact-control-button ${showCode ? 'active' : ''}`}
            onClick={toggleView}
            title={showCode ? 'Show rendered view' : 'Show source code'}
            aria-label={showCode ? 'Show rendered view' : 'Show source code'}
          >
            {showCode ? '👁️' : '</>'}
          </button>
          
        </div>
      </div>

      {/* Content area */}
      <div className="artifact-content">
        {renderContent()}
      </div>
    </div>
  );
};

export default ArtifactRenderer; 