// Bang Language VS Code Extension
const vscode = require('vscode');
const path = require('path');

function activate(context) {
    // bang run — 현재 파일 실행
    const runCmd = vscode.commands.registerCommand('bang.runFile', () => {
        const editor = vscode.window.activeTextEditor;
        if (!editor) {
            vscode.window.showErrorMessage('실행할 .bang 파일을 열어주세요.');
            return;
        }
        if (editor.document.languageId !== 'bang') {
            vscode.window.showErrorMessage('.bang 파일이 아닙니다.');
            return;
        }

        // 저장 후 실행
        editor.document.save().then(() => {
            const config = vscode.workspace.getConfiguration('bang');
            const exe = config.get('executablePath', 'bang');
            const mode = config.get('runMode', 'vm');
            const filePath = editor.document.fileName;

            const modeFlag = mode === 'interp' ? '--interp ' : '';
            const terminal = getOrCreateTerminal();
            terminal.show(true);
            terminal.sendText(`${exe} run ${modeFlag}"${filePath}"`);
        });
    });

    // bang check — 오류 검사
    const checkCmd = vscode.commands.registerCommand('bang.checkFile', () => {
        const editor = vscode.window.activeTextEditor;
        if (!editor || editor.document.languageId !== 'bang') {
            vscode.window.showErrorMessage('.bang 파일을 열어주세요.');
            return;
        }

        editor.document.save().then(() => {
            const config = vscode.workspace.getConfiguration('bang');
            const exe = config.get('executablePath', 'bang');
            const filePath = editor.document.fileName;

            const terminal = getOrCreateTerminal();
            terminal.show(true);
            terminal.sendText(`${exe} check "${filePath}"`);
        });
    });

    context.subscriptions.push(runCmd, checkCmd);
}

function getOrCreateTerminal() {
    const name = 'Bang';
    const existing = vscode.window.terminals.find(t => t.name === name);
    if (existing) return existing;
    return vscode.window.createTerminal({ name });
}

function deactivate() {}

module.exports = { activate, deactivate };
