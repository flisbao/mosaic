"""Respostas falsas da API de builds do Azure DevOps, usadas pelos testes e pelo servidor falso.

Três pipelines em dois projetos, com durações fáceis de somar:
  Alpha/Build API (definição 1): builds 101..103 com 1200 s de média e fila média de 60 s.
  Alpha/Deploy (definição 2): builds 201 e 202 com 480 s.
  Beta/Lint (definição 3): build 301 com 45 s.
O build 104 (Build API) foi cancelado e não conta.
"""

from datetime import datetime, timedelta, timezone

PROJECT_ALPHA = "Alpha"
PROJECT_BETA = "Beta"
ORGANIZATION = "org"
FIXTURE_NOW = datetime(2026, 10, 22, 12, 0, tzinfo=timezone.utc)
FIRST_BUILD_START = datetime(2026, 10, 20, 8, 0, tzinfo=timezone.utc)
BUILD_API_ID = 1
DEPLOY_ID = 2
LINT_ID = 3
CANCELED_BUILD_ID = 104
SECONDS_BETWEEN_BUILDS = 3600

BUILD_SPECS = (
    # id, projeto, definição, nome, fila (s), duração (s), resultado
    (101, PROJECT_ALPHA, BUILD_API_ID, "Build API", 60, 1200, "succeeded"),
    (102, PROJECT_ALPHA, BUILD_API_ID, "Build API", 30, 1500, "failed"),
    (103, PROJECT_ALPHA, BUILD_API_ID, "Build API", 90, 900, "succeeded"),
    (CANCELED_BUILD_ID, PROJECT_ALPHA, BUILD_API_ID, "Build API", 10, 20, "canceled"),
    (201, PROJECT_ALPHA, DEPLOY_ID, "Deploy", 0, 480, "succeeded"),
    (202, PROJECT_ALPHA, DEPLOY_ID, "Deploy", 0, 480, "succeeded"),
    (301, PROJECT_BETA, LINT_ID, "Lint", 5, 45, "succeeded"),
)

# build -> (job, tarefa, duração em segundos)
STEP_SPECS = {
    101: (("Compile", "dotnet build", 600), ("Compile", "Restore", 100), ("Test", "Unit tests", 400)),
    102: (("Compile", "dotnet build", 900), ("Compile", "Restore", 100), ("Test", "Unit tests", 400)),
    103: (("Compile", "dotnet build", 300), ("Compile", "Restore", 100), ("Test", "Unit tests", 400)),
    201: (("Release", "Publish", 300), ("Release", "Verify", 100)),
    202: (("Release", "Publish", 300), ("Release", "Verify", 100)),
    301: (("Quality", "Run lint", 40),),
}


def iso(moment):
    return moment.strftime("%Y-%m-%dT%H:%M:%S.0000000Z")


def build_start(build_id):
    position = [spec[0] for spec in BUILD_SPECS].index(build_id)
    return FIRST_BUILD_START + timedelta(seconds=SECONDS_BETWEEN_BUILDS * position)


def raw_build(build_id, project, definition_id, name, queue_seconds, duration_seconds, result):
    started = build_start(build_id)
    return {
        "id": build_id,
        "buildNumber": f"2026.{build_id}",
        "status": "completed",
        "result": result,
        "queueTime": iso(started - timedelta(seconds=queue_seconds)),
        "startTime": iso(started),
        "finishTime": iso(started + timedelta(seconds=duration_seconds)),
        "definition": {"id": definition_id, "name": name},
        "project": {"name": project},
        "_links": {"web": {"href": f"https://dev.azure.com/{ORGANIZATION}/{project}/_build/results?buildId={build_id}"}},
    }


def builds_of(project):
    return [raw_build(*spec) for spec in BUILD_SPECS if spec[1] == project]


def raw_projects():
    return {"count": 2, "value": [{"name": PROJECT_ALPHA}, {"name": PROJECT_BETA}]}


def timeline_record(record_id, parent_id, record_type, name, started=None, finished=None):
    return {
        "id": record_id,
        "parentId": parent_id,
        "type": record_type,
        "name": name,
        "startTime": iso(started) if started else None,
        "finishTime": iso(finished) if finished else None,
        "result": "succeeded",
    }


def raw_timeline(build_id):
    started = build_start(build_id)
    records = [
        timeline_record("stage", None, "Stage", "Build", started, started + timedelta(hours=1)),
        timeline_record("phase", "stage", "Phase", "Phase 1", started, started + timedelta(hours=1)),
        timeline_record("init", "phase", "Task", "Initialize job"),
    ]
    jobs_seen = {}
    for index, (job, task, seconds) in enumerate(STEP_SPECS.get(build_id, ())):
        job_id = jobs_seen.setdefault(job, f"job-{len(jobs_seen)}")
        if not any(record["id"] == job_id for record in records):
            records.append(timeline_record(job_id, "phase", "Job", job, started, started + timedelta(hours=1)))
        records.append(
            timeline_record(f"task-{index}", job_id, "Task", task, started, started + timedelta(seconds=seconds))
        )
    return {"records": records}
