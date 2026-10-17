#!/usr/bin/env node
// Plugin de exemplo em Node: imprime um snapshot JSON no stdout.

function main(argumentsList) {
  if (argumentsList.length !== 1 || argumentsList[0] !== "snapshot") {
    console.error("este exemplo só implementa 'snapshot'");
    return 2;
  }
  const snapshot = {
    summary: {
      title: "Exemplo Node",
      lines: [
        { text: "olá do plugin em Node", flag: "updated" },
        { text: "troque este script pelo seu" },
      ],
    },
  };
  console.log(JSON.stringify(snapshot));
  return 0;
}

process.exit(main(process.argv.slice(2)));
