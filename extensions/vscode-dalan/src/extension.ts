// ============================================================
// Dalin L — VSCode Extension
// LSP Client that connects VSCode editor ↔ dalin-ls server
// ============================================================

import * as vscode from 'vscode';
import {
    LanguageClient,
    LanguageClientOptions,
    ServerOptions,
    TransportKind
} from 'vscode-languageclient/node';

let client: LanguageClient;

// Seven-channel attributes for autocompletion
const SEVEN_CHANNEL_ATTRS: vscode.CompletionItem[] = [
    { label: '@pure', kind: vscode.CompletionItemKind.Keyword, detail: 'Pure function (no side effects)' },
    { label: '@io', kind: vscode.CompletionItemKind.Keyword, detail: 'IO effect channel' },
    { label: '@cpu', kind: vscode.CompletionItemKind.Keyword, detail: 'CPU capability channel' },
    { label: '@net', kind: vscode.CompletionItemKind.Keyword, detail: 'Networking channel' },
    { label: '@perceive', kind: vscode.CompletionItemKind.Keyword, detail: 'Perceive cognitive loop' },
    { label: '@observe', kind: vscode.CompletionItemKind.Keyword, detail: 'Observe cognitive loop' },
    { label: '@reflect', kind: vscode.CompletionItemKind.Keyword, detail: 'Reflect cognitive loop' },
    { label: '@act', kind: vscode.CompletionItemKind.Keyword, detail: 'Act cognitive loop' },
    { label: '@verified', kind: vscode.CompletionItemKind.Keyword, detail: 'Verified function (high confidence)' },
    { label: '@test', kind: vscode.CompletionItemKind.Keyword, detail: 'Test function' },
    { label: '@bench', kind: vscode.CompletionItemKind.Keyword, detail: 'Benchmark function' },
    { label: '@latency(ms)', kind: vscode.CompletionItemKind.Keyword, detail: 'Latency constraint in ms' },
    { label: '@gov(phase)', kind: vscode.CompletionItemKind.Keyword, detail: 'Governance phase' },
    { label: '@llm(prompt)', kind: vscode.CompletionItemKind.Keyword, detail: 'LLM-generated function body' },
];

// Dalin L built-in type completion items
const DALAN_TYPE_ITEMS: vscode.CompletionItem[] = [
    { label: 'int', kind: vscode.CompletionItemKind.TypeParameter, detail: 'Integer type' },
    { label: 'float', kind: vscode.CompletionItemKind.TypeParameter, detail: 'Floating point type' },
    { label: 'string', kind: vscode.CompletionItemKind.TypeParameter, detail: 'String type' },
    { label: 'bool', kind: vscode.CompletionItemKind.TypeParameter, detail: 'Boolean type' },
    { label: 'void', kind: vscode.CompletionItemKind.TypeParameter, detail: 'Void type' },
    { label: 'option', kind: vscode.CompletionItemKind.TypeParameter, detail: 'Option type' },
    { label: 'result', kind: vscode.CompletionItemKind.TypeParameter, detail: 'Result type' },
    { label: 'list', kind: vscode.CompletionItemKind.TypeParameter, detail: 'List type' },
    { label: 'map', kind: vscode.CompletionItemKind.TypeParameter, detail: 'Map type' },
];

// Dalin L snippet completion items
const DALAN_SNIPPETS: vscode.CompletionItem[] = [
    {
        label: 'fn',
        kind: vscode.CompletionItemKind.Snippet,
        detail: 'Function declaration',
        insertText: new vscode.SnippetString('fn ${1:name}($2) @pure @cpu {\n    return ${3:null}\n}'),
    },
    {
        label: 'if',
        kind: vscode.CompletionItemKind.Snippet,
        detail: 'If expression',
        insertText: new vscode.SnippetString('if ${1:condition} {\n    ${2}\n} else {\n    ${3}\n}'),
    },
    {
        label: 'for',
        kind: vscode.CompletionItemKind.Snippet,
        detail: 'For loop',
        insertText: new vscode.SnippetString('for ${1:item} in ${2:list} {\n    ${3}\n}'),
    },
    {
        label: 'while',
        kind: vscode.CompletionItemKind.Snippet,
        detail: 'While loop',
        insertText: new vscode.SnippetString('while ${1:condition} {\n    ${2}\n}'),
    },
    {
        label: 'let',
        kind: vscode.CompletionItemKind.Snippet,
        detail: 'Variable binding',
        insertText: new vscode.SnippetString('let ${1:name} = ${2:value}'),
    },
    {
        label: 'match',
        kind: vscode.CompletionItemKind.Snippet,
        detail: 'Pattern matching',
        insertText: new vscode.SnippetString('match ${1:value} {\n    ${2:pattern} => ${3:result}\n    _ => ${4}\n}'),
    },
    {
        label: 'async',
        kind: vscode.CompletionItemKind.Snippet,
        detail: 'Async function',
        insertText: new vscode.SnippetString('async fn ${1:name}($2) @net {\n    return ${3:null}\n}'),
    },
    {
        label: 'test',
        kind: vscode.CompletionItemKind.Snippet,
        detail: 'Test function',
        insertText: new vscode.SnippetString('fn test_${1:name}() @test @cpu {\n    assert(${2:true})\n}'),
    },
];

// Weak cache for definition lookups
const definitionCache = new Map<string, vscode.Location[]>();

export function activate(context: vscode.ExtensionContext) {
    // ── Determine LSP server path ──
    const config = vscode.workspace.getConfiguration('dalan');
    let serverModule: string;

    if (config.get('languageServer.path')) {
        serverModule = config.get('languageServer.path')!;
    } else {
        try {
            serverModule = require.resolve('@dalib/dalin-ls', { paths: [process.cwd()] });
        } catch {
            serverModule = 'dalin-ls';
        }
    }

    // ── Server options ──
    const serverOptions: ServerOptions = {
        command: serverModule,
        args: ['--stdio'],
        transport: TransportKind.stdio
    };

    // ── Client options ──
    const clientOptions: LanguageClientOptions = {
        documentSelector: [{ scheme: 'file', language: 'dalan' }],
        synchronize: {
            fileEvents: vscode.workspace.createFileSystemWatcher('**/*.dal')
        },
        middleware: {
            provideHover(document, position, token, next) {
                return next(document, position, token);
            },
            resolveCompletionItem(item, token, next) {
                if (item.kind === vscode.CompletionItemKind.Keyword) {
                    item.detail = 'Dalin L keyword';
                }
                return next(item, token);
            }
        },
        initializationOptions: {
            showSevenChannelInfo: config.get('showSevenChannelInfo') ?? true,
            autoCompleteOnAtSymbol: config.get('autoComplete.onAtSymbol') ?? true
        }
    };

    // ── Create & start client ──
    client = new LanguageClient(
        'dalan-language-server',
        'Dalin L Language Server',
        serverOptions,
        clientOptions
    );

    // ── Register inline completions for @attributes ──
    const attrCompletionProvider = vscode.languages.registerCompletionItemProvider(
        { language: 'dalan', scheme: 'file' },
        {
            provideCompletionItems(document: vscode.TextDocument, position: vscode.Position) {
                const linePrefix = document.lineAt(position).text.substring(0, position.character);

                // If typing '@', show channel attributes
                if (linePrefix.endsWith('@')) {
                    return SEVEN_CHANNEL_ATTRS;
                }

                // If starting a new line or after whitespace/brackets, suggest snippets
                const trimmed = linePrefix.trimEnd();
                if (trimmed.length === 0 || /[{\s,;(]$/.test(linePrefix)) {
                    return [...DALAN_SNIPPETS, ...DALAN_TYPE_ITEMS];
                }

                return undefined;
            }
        },
        '@',  // Trigger character
        ' '   // Also trigger on space for type hints
    );

    // ── Register go-to-definition provider ──
    const definitionProvider = vscode.languages.registerDefinitionProvider(
        { language: 'dalan', scheme: 'file' },
        {
            provideDefinition(document: vscode.TextDocument, position: vscode.Position) {
                const wordRange = document.getWordRangeAtPosition(position);
                if (!wordRange) return null;

                const word = document.getText(wordRange);
                const text = document.getText();
                const line = document.lineAt(position);

                // Check if this is a function call (identifier followed by '(')
                const charAfter = line.text.substring(position.character + word.length, position.character + word.length + 1);
                if (charAfter !== '(') return null;

                // Simple search: find the function definition in the document
                const fnRegex = new RegExp(`\\bfn\\s+${word}\\b`);
                const match = text.match(fnRegex);
                if (match) {
                    const offset = match.index!;
                    const defPos = document.positionAt(offset);
                    return new vscode.Location(document.uri, defPos);
                }

                return null;
            }
        }
    );

    // ── Register commands ──
    context.subscriptions.push(
        vscode.commands.registerCommand('dalan.restartLsp', () => {
            client.restart();
            vscode.window.showInformationMessage('Dalin L: Restarted language server');
        }),
        vscode.commands.registerCommand('dalan.compile', async () => {
            const editor = vscode.window.activeTextEditor;
            if (!editor) return;

            const filePath = editor.document.fileName;
            const outputChannel = vscode.window.createOutputChannel('Dalin L: Compile');
            outputChannel.appendLine(`Compiling ${filePath}...`);

            try {
                const { execSync } = require('child_process');
                const result = execSync(`dalib compile "${filePath}"`, {
                    encoding: 'utf8',
                    timeout: 30000
                });
                outputChannel.appendLine(result);
                outputChannel.show();
                vscode.window.showInformationMessage('Dalin L: Compilation successful');
            } catch (err: any) {
                outputChannel.appendLine(`Error: ${err.message}`);
                outputChannel.show();
                vscode.window.showErrorMessage('Dalin L: Compilation failed');
            }
        }),
        vscode.commands.registerCommand('dalan.run', async () => {
            const editor = vscode.window.activeTextEditor;
            if (!editor) return;

            const filePath = editor.document.fileName;
            const outputChannel = vscode.window.createOutputChannel('Dalin L: Run');
            outputChannel.appendLine(`Running ${filePath}...`);

            try {
                const { execSync } = require('child_process');
                const result = execSync(`dalib run "${filePath}"`, {
                    encoding: 'utf8',
                    timeout: 30000
                });
                outputChannel.appendLine(result);
                outputChannel.show();
            } catch (err: any) {
                outputChannel.appendLine(`Error: ${err.message}`);
                outputChannel.show();
                vscode.window.showErrorMessage('Dalin L: Execution failed');
            }
        }),
        vscode.commands.registerCommand('dalan.formatDocument', async () => {
            const editor = vscode.window.activeTextEditor;
            if (!editor || editor.document.languageId !== 'dalan') return;

            const filePath = editor.document.fileName;

            try {
                const { execSync } = require('child_process');
                const result = execSync(`dalib fmt "${filePath}"`, {
                    encoding: 'utf8',
                    timeout: 30000
                });

                // Replace document content with formatted version
                const fullRange = new vscode.Range(
                    editor.document.positionAt(0),
                    editor.document.positionAt(editor.document.getText().length)
                );
                await editor.edit(edit => edit.replace(fullRange, result));
                vscode.window.showInformationMessage('Dalin L: Document formatted');
            } catch (err: any) {
                vscode.window.showErrorMessage(`Format failed: ${err.message}`);
            }
        }),
        vscode.commands.registerCommand('dalan.goToDefinition', async () => {
            const editor = vscode.window.activeTextEditor;
            if (!editor) return;

            const position = editor.selection.active;
            const wordRange = editor.document.getWordRangeAtPosition(position);
            if (!wordRange) return;

            const word = editor.document.getText(wordRange);
            const text = editor.document.getText();
            const fnRegex = new RegExp(`\\bfn\\s+${word}\\b`);
            const match = text.match(fnRegex);

            if (match) {
                const offset = match.index!;
                const defPos = editor.document.positionAt(offset);
                editor.selection = new vscode.Selection(defPos, defPos);
                editor.revealRange(new vscode.Range(defPos, defPos));
            } else {
                vscode.window.showWarningMessage(`Definition not found for: ${word}`);
            }
        }),
        vscode.commands.registerCommand('dalan.runTests', async () => {
            const editor = vscode.window.activeTextEditor;
            if (!editor) return;

            const filePath = editor.document.fileName;
            const outputChannel = vscode.window.createOutputChannel('Dalin L: Tests');
            outputChannel.appendLine(`Running tests in ${filePath}...`);

            try {
                const { execSync } = require('child_process');
                const result = execSync(`dalib test "${filePath}"`, {
                    encoding: 'utf8',
                    timeout: 60000
                });
                outputChannel.appendLine(result);
                outputChannel.show();
                vscode.window.showInformationMessage('Dalin L: Tests completed');
            } catch (err: any) {
                outputChannel.appendLine(`Error: ${err.message}`);
                outputChannel.show();
            }
        }),
        vscode.commands.registerCommand('dalan.initProject', async () => {
            const uri = await vscode.window.showOpenDialog({
                canSelectFolders: true,
                canSelectFiles: false,
                openLabel: 'Initialize Project Here'
            });

            if (!uri || !uri[0]) return;

            const projectName = await vscode.window.showInputBox({
                prompt: 'Enter project name',
                placeHolder: 'my-agent-project'
            });

            if (!projectName) return;

            const targetDir = uri[0].fsPath;
            try {
                const { execSync } = require('child_process');
                execSync(`dalib pkg init --name ${projectName}`, { cwd: targetDir });
                vscode.window.showInformationMessage(`Dalin L: Project initialized at ${targetDir}`);

                vscode.workspace.openWorkspaceFolder(uri[0]);
            } catch (err: any) {
                vscode.window.showErrorMessage(`Failed to initialize project: ${err.message}`);
            }
        })
    );

    // ── Register diagnostics (lint) on save ──
    const diagnosticCollection = vscode.languages.createDiagnosticCollection('dalan');
    context.subscriptions.push(diagnosticCollection);

    context.subscriptions.push(
        vscode.workspace.onDidSaveTextDocument(async (doc) => {
            if (doc.languageId !== 'dalan') return;

            const filePath = doc.fileName;
            try {
                const { execSync } = require('child_process');
                const result = execSync(`dalib compile "${filePath}"`, {
                    encoding: 'utf8',
                    timeout: 30000
                });

                // Clear diagnostics on success
                diagnosticCollection.set(doc.uri, []);
            } catch (err: any) {
                // Parse error output for diagnostics
                const stderr = err.stderr || err.message || '';
                const diagnostics: vscode.Diagnostic[] = [];

                const errorLineMatch = stderr.match(/\[(\d+):(\d+)\]\s*(.+)/g);
                if (errorLineMatch) {
                    for (const line of errorLineMatch) {
                        const parts = line.match(/\[(\d+):(\d+)\]\s*(.+)/);
                        if (parts) {
                            const [, lineStr, colStr, message] = parts;
                            const lineNum = parseInt(lineStr) - 1;
                            const colNum = parseInt(colStr) - 1;
                            const range = new vscode.Range(lineNum, colNum, lineNum, colNum + 10);
                            diagnostics.push(new vscode.Diagnostic(
                                range,
                                message.trim(),
                                vscode.DiagnosticSeverity.Error
                            ));
                        }
                    }
                }

                if (diagnostics.length === 0) {
                    // Add generic error
                    const range = new vscode.Range(0, 0, 0, 10);
                    diagnostics.push(new vscode.Diagnostic(
                        range,
                        stderr.substring(0, 200),
                        vscode.DiagnosticSeverity.Error
                    ));
                }

                diagnosticCollection.set(doc.uri, diagnostics);
            }
        })
    );

    context.subscriptions.push(attrCompletionProvider);
    context.subscriptions.push(definitionProvider);

    client.start();
    vscode.window.showInformationMessage('Dalin L: Language server started');
}

export function deactivate(): Thenable<void> | undefined {
    if (!client) {
        return undefined;
    }
    return client.stop();
}
