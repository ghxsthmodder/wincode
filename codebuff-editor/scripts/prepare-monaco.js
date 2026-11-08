// Baixa o Monaco Editor (npm) e deixa os assets estáticos em assets/monaco/vs.
// Estrutura final esperada pelo editor:
//   assets/monaco/vs/loader.js
//   assets/monaco/vs/editor/editor.main.js
//   assets/monaco/vs/assets/*.worker-*.js
const fs = require('fs');
const https = require('https');
const { execSync } = require('child_process');

const MONACO_VERSION = '0.57.0';
const TAR_URL = `https://registry.npmjs.org/monaco-editor/-/monaco-editor-${MONACO_VERSION}.tgz`;
const DEST_DIR = `${process.cwd()}/assets/monaco`;
const CACHE_TGZ = `.cache/monaco-editor-${MONACO_VERSION}.tgz`;

const download = (url) =>
  new Promise((resolve, reject) => {
    const request = https.get(url, (res) => {
      if (res.statusCode !== 200) {
        reject(new Error(`HTTP ${res.statusCode} em ${url}`));
        return;
      }
      const chunks = [];
      res.on('data', (chunk) => chunks.push(chunk));
      res.on('end', () => resolve(Buffer.concat(chunks)));
    });
    request.on('error', reject);
  });

const copyDir = (src, dst) => {
  fs.mkdirSync(dst, { recursive: true });
  for (const entry of fs.readdirSync(src, { withFileTypes: true })) {
    const from = `${src}/${entry.name}`;
    const to = `${dst}/${entry.name}`;
    if (entry.isDirectory()) copyDir(from, to);
    else fs.copyFileSync(from, to);
  }
};

(async () => {
  fs.mkdirSync(DEST_DIR, { recursive: true });
  fs.mkdirSync('.cache', { recursive: true });

  let archive;
  if (fs.existsSync(CACHE_TGZ)) {
    console.log('usando cache', CACHE_TGZ);
    archive = CACHE_TGZ;
  } else {
    console.log('baixando', TAR_URL);
    fs.writeFileSync(CACHE_TGZ, await download(TAR_URL));
    archive = CACHE_TGZ;
  }

  const tmpDir = fs.mkdtempSync('.cache/extract-');
  execSync(`tar -xzf ${archive} -C ${tmpDir}`);

  const packageDir = `${tmpDir}/package`;
  const minVs = `${packageDir}/min/vs`;
  if (!fs.existsSync(minVs)) {
    throw new Error('min/vs não encontrado no pacote do monaco-editor');
  }

  // min/vs contém loader.js, editor/, language/, assets/*.worker-*.js
  // Copiamos o conteúdo de min/vs para assets/monaco/vs (sem duplicar a pasta vs).
  if (fs.existsSync(`${DEST_DIR}/vs`)) {
    fs.rmSync(`${DEST_DIR}/vs`, { recursive: true });
  }
  copyDir(minVs, `${DEST_DIR}/vs`);

  if (fs.existsSync(`${packageDir}/monaco.d.ts`)) {
    fs.copyFileSync(`${packageDir}/monaco.d.ts`, `${DEST_DIR}/monaco.d.ts`);
  }

  fs.writeFileSync(
    `${DEST_DIR}/README.md`,
    `Monaco Editor ${MONACO_VERSION} (MIT), assets estáticos usados pelo CodeBuff Editor.\n` +
      `Gerado por scripts/prepare-monaco.js — não editar à mão.\n`,
  );

  fs.rmSync(tmpDir, { recursive: true, force: true });
  console.log('Monaco pronto em', `${DEST_DIR}/vs`);
})().catch((err) => {
  console.error(err);
  process.exit(1);
});
