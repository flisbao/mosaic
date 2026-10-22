#!/usr/bin/env node
// Galeria do mosaic: um widget com todos os blocos do protocolo v1, a agenda completa no
// detalhe (Enter) e um aviso por hora.
// Não usa rede. Serve de modelo para escrever o seu widget; valide com
// `mosaic check node plugins/gallery.js`.

const PROTOCOL_VERSION = 1;

function agendaRows() {
  return [
    { time: "09:00", text: "Daily do time", tone: "muted" },
    {
      time: "10:30",
      text: "Revisão de código",
      tone: "current",
      tags: [{ text: "agora", tone: "ok" }],
    },
    {
      time: "14:00",
      text: "1:1 com a gestão",
      flag: "updated",
      tags: [{ text: "sala 3" }, { text: "remoto", tone: "muted" }],
    },
  ];
}

const AGENDA_DETAIL_HEADING = "Agenda de hoje";
const FULL_AGENDA = [
  "09:00 Daily do time",
  "10:30 Revisão de código",
  "12:00 Almoço",
  "14:00 1:1 com a gestão",
  "15:30 Planejamento da sprint",
  "16:30 Estudo de ownership em Rust",
  "17:30 Fechar o dia",
];

function agendaDetail() {
  return [
    {
      heading: AGENDA_DETAIL_HEADING,
      items: FULL_AGENDA.map((entry) => ({ text: entry })),
    },
  ];
}

function teamTable() {
  return {
    type: "table",
    columns: [
      { title: "Pessoa" },
      { title: "Em andamento", align: "right" },
      { title: "Estado" },
    ],
    rows: [
      ["Ana", "3", { text: "ok", tone: "ok" }],
      ["Bruno", "5", { text: "parado", tone: "danger", flag: "attention" }],
      ["Carla", "2", { text: "revisão", tone: "warn" }],
    ],
  };
}

function hourlyAlertId(now) {
  const pad = (number) => String(number).padStart(2, "0");
  const day = `${now.getFullYear()}${pad(now.getMonth() + 1)}${pad(now.getDate())}`;
  return `gallery-${day}${pad(now.getHours())}`;
}

function buildSnapshot(now) {
  return {
    protocol: PROTOCOL_VERSION,
    summary: {
      title: "Galeria",
      blocks: [
        { type: "rows", rows: agendaRows() },
        {
          type: "stat",
          items: [
            { value: "3", label: "reuniões", tone: "warn" },
            { value: "2h", label: "livre", tone: "ok" },
            { value: "7", label: "tarefas" },
          ],
          hint: "dados de exemplo, sem rede",
        },
        teamTable(),
        {
          type: "lines",
          lines: [
            { text: "tudo em dia", tone: "ok" },
            { text: "veja PROTOCOL.md", tone: "muted" },
          ],
        },
      ],
    },
    detail: agendaDetail(),
    alerts: [
      {
        id: hourlyAlertId(now),
        title: "Aviso da galeria",
        body: "Um aviso novo por hora, só para demonstrar.",
        level: "info",
      },
    ],
  };
}

function main(argumentsList) {
  if (argumentsList.length !== 1 || argumentsList[0] !== "snapshot") {
    console.error("a galeria só implementa 'snapshot'");
    return 2;
  }
  console.log(JSON.stringify(buildSnapshot(new Date())));
  return 0;
}

process.exit(main(process.argv.slice(2)));
