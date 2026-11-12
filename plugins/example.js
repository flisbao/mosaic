#!/usr/bin/env node
// Plugin de exemplo em Node: imprime um snapshot JSON no stdout.

const LANGUAGE_VARIABLE = "MOSAIC_LANGUAGE";
const LANGUAGE_ENGLISH = "en";
const LANGUAGE_PORTUGUESE = "pt-BR";
const PORTUGUESE_PREFIX = "pt";

const MESSAGES = {
  [LANGUAGE_PORTUGUESE]: {
    title: "Exemplo Node",
    greeting: "olá do plugin em Node",
    replaceHint: "troque este script pelo seu",
    snapshotOnly: "este exemplo só implementa 'snapshot'",
  },
  [LANGUAGE_ENGLISH]: {
    title: "Node example",
    greeting: "hello from the Node plugin",
    replaceHint: "replace this script with yours",
    snapshotOnly: "this example only implements 'snapshot'",
  },
};

function resolveLanguage(environment) {
  const requested = (environment[LANGUAGE_VARIABLE] || "").trim().toLowerCase();
  return requested.startsWith(PORTUGUESE_PREFIX) ? LANGUAGE_PORTUGUESE : LANGUAGE_ENGLISH;
}

function buildSnapshot(messages) {
  return {
    summary: {
      title: messages.title,
      blocks: [
        {
          type: "lines",
          lines: [
            { text: messages.greeting, flag: "updated" },
            { text: messages.replaceHint },
          ],
        },
      ],
    },
  };
}

function main(argumentsList, environment) {
  const messages = MESSAGES[resolveLanguage(environment)];
  if (argumentsList.length !== 1 || argumentsList[0] !== "snapshot") {
    console.error(messages.snapshotOnly);
    return 2;
  }
  console.log(JSON.stringify(buildSnapshot(messages)));
  return 0;
}

process.exit(main(process.argv.slice(2), process.env));
