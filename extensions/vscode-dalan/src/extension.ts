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

export function activate(context: vscode.ExtensionContext) {
    // ── Determine LSP server path ──
    const config = vscode.workspace.getConfiguration('dalan');
    let serverModule: string;
    
    if (config.get('languageServer.path')) {
        serverModule = config.get('languageServer.path')!;
    } else {
        // Try to find dalin-ls in PATH or project node_modules/.bin
        try {
            serverModule = require.resolve('@dalib/dalin-ls', { paths: [process.cwd()] });
        } catch {
            // Default: expect it in system PATH
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
            // Notify server about file changes
            fileEvents: vscode.workspace.createFileSystemWatcher('**/*.dal')
        },
        middleware: {
            // Custom handling for seven-channel hover info
            provideHover(document, position, token, next) {
                return next(document, position, token);
            },
            // Intercept completion to add @attributes
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

    context.subscriptions.push(
        vscode.commands.registerCommand('dalan.restartLsp', () => {
            client.restart();
            vscode.window.showInformationMessage('Dalin L: Restarted language server');
        }),
        vscode.commands.registerCommand('dalan.showDiagnostics', () => {
            const editor = vscode.window.activeTextEditor;
            if (!editor) {
                vscode.window.showInformationMessage('Dalin L: No active editor');
                return;
            }
            vscode.window.showInformationMessage('Dalin L: Diagnostics shown in Problems panel');
        }),
        vscode.commands.registerCommand('dalan.compile', async () => {
            const editor = vscode.window.activeTextEditor;
            if (!editor) return;
            
            const filePath = editor.document.fileName;
            const outputChannel = vscode.window.createOutputChannel('Dalin L: Compile');
            outputChannel.appendLine(`Compiling ${filePath}...`);
            
            try {
                // Execute dalib compile via child_process
                const { execFileSync } = require('child_process');
                const result = execFileSync('dalib', ['compile', filePath], { 
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
                const { execFileSync } = require('child_process');
                execFileSync('dalib', ['pkg', 'init', '--name', projectName], { cwd: targetDir });
                vscode.window.showInformationMessage(`Dalin L: Project initialized at ${targetDir}`);
                
                // Open new project
                vscode.workspace.openWorkspaceFolder(uri[0]);
            } catch (err: any) {
                vscode.window.showErrorMessage(`Failed to initialize project: ${err.message}`);
            }
        }),
        vscode.commands.registerCommand('dalan.run', async () => {
            const editor = vscode.window.activeTextEditor;
            if (!editor) return;
            
            const filePath = editor.document.fileName;
            const outputChannel = vscode.window.createOutputChannel('Dalin L: Run');
            outputChannel.appendLine(`Running ${filePath}...`);
            
            try {
                const { execFileSync } = require('child_process');
                const result = execFileSync('dalib', ['run', filePath], {
                    encoding: 'utf8',
                    timeout: 30000
                });
                outputChannel.appendLine(result);
                outputChannel.show();
                vscode.window.showInformationMessage('Dalin L: Execution successful');
            } catch (err: any) {
                outputChannel.appendLine(`Error: ${err.message}`);
                outputChannel.show();
                vscode.window.showErrorMessage('Dalin L: Execution failed');
            }
        })
    );

    client.start();
    vscode.window.showInformationMessage('Dalin L: Language server started');
}

export function deactivate(): Thenable<void> | undefined {
    if (!client) {
        return undefined;
    }
    return client.stop();
}
