#!/usr/bin/env node
// Galeria do mosaic: um widget com todos os blocos do protocolo v1, a agenda completa no
// detalhe (Enter) e um aviso por hora.
// Não usa rede. Serve de modelo para escrever o seu widget; valide com
// `mosaic check node plugins/gallery.js`.

const PROTOCOL_VERSION = 1;
const LANGUAGE_VARIABLE = "MOSAIC_LANGUAGE";
const LANGUAGE_ENGLISH = "en";
const LANGUAGE_PORTUGUESE = "pt-BR";
const PORTUGUESE_PREFIX = "pt";

const MESSAGES = {
  [LANGUAGE_PORTUGUESE]: {
    title: "Galeria",
    agendaDaily: "Daily do time",
    agendaCodeReview: "Revisão de código",
    agendaNowTag: "agora",
    agendaOneOnOne: "1:1 com a gestão",
    agendaRoomTag: "sala 3",
    agendaRemoteTag: "remoto",
    agendaDetailHeading: "Agenda de hoje",
    fullAgenda: [
      "09:00 Daily do time",
      "10:30 Revisão de código",
      "12:00 Almoço",
      "14:00 1:1 com a gestão",
      "15:30 Planejamento da sprint",
      "16:30 Estudo de ownership em Rust",
      "17:30 Fechar o dia",
    ],
    tablePerson: "Pessoa",
    tableInProgress: "Em andamento",
    tableState: "Estado",
    stateOk: "ok",
    stateStuck: "parado",
    stateReview: "revisão",
    statMeetings: "reuniões",
    statFree: "livre",
    statTasks: "tarefas",
    statHint: "dados de exemplo, sem rede",
    lineAllGood: "tudo em dia",
    lineSeeProtocol: "veja PROTOCOL.md",
    alertTitle: "Aviso da galeria",
    alertBody: "Um aviso novo por hora, só para demonstrar.",
    snapshotOnly: "a galeria só implementa 'snapshot'",
  },
  [LANGUAGE_ENGLISH]: {
    title: "Gallery",
    agendaDaily: "Team daily",
    agendaCodeReview: "Code review",
    agendaNowTag: "now",
    agendaOneOnOne: "1:1 with management",
    agendaRoomTag: "room 3",
    agendaRemoteTag: "remote",
    agendaDetailHeading: "Today's agenda",
    fullAgenda: [
      "09:00 Team daily",
      "10:30 Code review",
      "12:00 Lunch",
      "14:00 1:1 with management",
      "15:30 Sprint planning",
      "16:30 Rust ownership study",
      "17:30 Wrap up the day",
    ],
    tablePerson: "Person",
    tableInProgress: "In progress",
    tableState: "State",
    stateOk: "ok",
    stateStuck: "stuck",
    stateReview: "review",
    statMeetings: "meetings",
    statFree: "free",
    statTasks: "tasks",
    statHint: "sample data, no network",
    lineAllGood: "all caught up",
    lineSeeProtocol: "see PROTOCOL.md",
    alertTitle: "Gallery alert",
    alertBody: "A new alert every hour, just for demonstration.",
    snapshotOnly: "the gallery only implements 'snapshot'",
  },
};

function resolveLanguage(environment) {
  const requested = (environment[LANGUAGE_VARIABLE] || "").trim().toLowerCase();
  return requested.startsWith(PORTUGUESE_PREFIX) ? LANGUAGE_PORTUGUESE : LANGUAGE_ENGLISH;
}

function agendaRows(messages) {
  return [
    { time: "09:00", text: messages.agendaDaily, tone: "muted" },
    {
      time: "10:30",
      text: messages.agendaCodeReview,
      tone: "current",
      tags: [{ text: messages.agendaNowTag, tone: "ok" }],
    },
    {
      time: "14:00",
      text: messages.agendaOneOnOne,
      flag: "updated",
      tags: [{ text: messages.agendaRoomTag }, { text: messages.agendaRemoteTag, tone: "muted" }],
    },
  ];
}

function agendaDetail(messages) {
  return [
    {
      heading: messages.agendaDetailHeading,
      items: messages.fullAgenda.map((entry) => ({ text: entry })),
    },
  ];
}

function teamTable(messages) {
  return {
    type: "table",
    columns: [
      { title: messages.tablePerson },
      { title: messages.tableInProgress, align: "right" },
      { title: messages.tableState },
    ],
    rows: [
      ["Ana", "3", { text: messages.stateOk, tone: "ok" }],
      ["Bruno", "5", { text: messages.stateStuck, tone: "danger", flag: "attention" }],
      ["Carla", "2", { text: messages.stateReview, tone: "warn" }],
    ],
  };
}

function hourlyAlertId(now) {
  const pad = (number) => String(number).padStart(2, "0");
  const day = `${now.getFullYear()}${pad(now.getMonth() + 1)}${pad(now.getDate())}`;
  return `gallery-${day}${pad(now.getHours())}`;
}

function buildSnapshot(now, messages) {
  return {
    protocol: PROTOCOL_VERSION,
    summary: {
      title: messages.title,
      blocks: [
        { type: "rows", rows: agendaRows(messages) },
        {
          type: "stat",
          items: [
            { value: "3", label: messages.statMeetings, tone: "warn" },
            { value: "2h", label: messages.statFree, tone: "ok" },
            { value: "7", label: messages.statTasks },
          ],
          hint: messages.statHint,
        },
        teamTable(messages),
        {
          type: "lines",
          lines: [
            { text: messages.lineAllGood, tone: "ok" },
            { text: messages.lineSeeProtocol, tone: "muted" },
          ],
        },
      ],
    },
    detail: agendaDetail(messages),
    alerts: [
      {
        id: hourlyAlertId(now),
        title: messages.alertTitle,
        body: messages.alertBody,
        level: "info",
      },
    ],
  };
}

function main(argumentsList, environment) {
  const messages = MESSAGES[resolveLanguage(environment)];
  if (argumentsList.length !== 1 || argumentsList[0] !== "snapshot") {
    console.error(messages.snapshotOnly);
    return 2;
  }
  console.log(JSON.stringify(buildSnapshot(new Date(), messages)));
  return 0;
}

process.exit(main(process.argv.slice(2), process.env));
