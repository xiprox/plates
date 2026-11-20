"""A probe of the task list as an MCP server: what Claude Code gives a stdio server it starts,
whether the server's instructions reach Claude, and whether Claude keeps a list by them.

Claude Code starts this script and speaks MCP to it over stdin and stdout, one JSON-RPC message
per line. The script logs each start, each message, and each exit to results/server-log.jsonl,
and keeps each session's list in results/tasks/<session>.json, beside this script.

It logs only the environment variables named in LOGGED_ENV, since the environment Claude Code
passes also holds the account's email address and tokens.

Usage: claude mcp add --scope local plates-tasks -- python3 -I <absolute path to this file>
"""

import datetime
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
# A test run points PLATES_PROBE_RESULTS elsewhere, so its output stays out of results/.
RESULTS = os.environ.get("PLATES_PROBE_RESULTS") or os.path.join(HERE, "results")
TASKS = os.path.join(RESULTS, "tasks")
LOG = os.path.join(RESULTS, "server-log.jsonl")

LOGGED_ENV = [
    "CLAUDE_CODE_HOST_SESSION_ID",
    "CLAUDE_CODE_SESSION_ID",
    "CLAUDE_CODE_ENTRYPOINT",
    "CLAUDE_CODE_DESKTOP_APP_VERSION",
    "CLAUDE_PID",
    "AI_AGENT",
]

STATES = ["todo", "doing", "waiting", "done", "dropped"]

# The marker lets a session prove it can read these instructions, by quoting it.
INSTRUCTIONS = """\
Probe marker: lantern-47.

Plates, the rack of session tiles beside the Claude app, shows this session's task list: the planned work that runs past the current request, so the user can see where each piece stands.

Start a list without being asked once you and the user agree on work that runs past the current request: at least two pieces of work, or one piece that's waiting on someone. The steps of the request you're working on don't count, since the app's own plan covers those.
When you start a list, say so in one line, such as "Started a Plates task list: 3 tasks."
If the user says not to keep a list in this session, call tasks_decline.

Keep the list to big pieces of work, about 3 to 10.
Update a task when its state changes or there's news on it, such as who it's waiting on, not at every step.
Mark a task dropped rather than leaving it stale.
Call tasks_replace only when the user asks to start over or to replace the list.
Call tasks_get before changing a list you haven't seen in this conversation.
This list is the only copy of the plan's tasks: don't copy it into your memory or any other notes, since a second copy goes stale.

The tools are tasks_get, task_add, task_update, tasks_replace, and tasks_decline, from the plates-tasks server.
"""

TASK_FIELDS = {
    "title": {"type": "string", "description": "The piece of work, in a few words."},
    "state": {
        "type": "string",
        "enum": STATES,
        "description": "todo, doing, waiting (on someone or something), done, or dropped.",
    },
    "status": {
        "type": "string",
        "description": "The latest on this task in one line, such as \"PR open; Sam reviewing the copy\". Replaces the previous status.",
    },
    "waiting_on": {
        "type": "string",
        "description": "Who or what the task waits on, when its state is waiting: \"you\" for the user, or a name.",
    },
}

TOOLS = [
    {
        "name": "tasks_get",
        "description": "Returns this session's Plates task list.",
        "inputSchema": {"type": "object", "properties": {}},
    },
    {
        "name": "task_add",
        "description": "Adds a task to this session's Plates task list, and starts the list if there's none.",
        "inputSchema": {
            "type": "object",
            "properties": TASK_FIELDS,
            "required": ["title"],
        },
    },
    {
        "name": "task_update",
        "description": "Changes a task's state, status, waiting_on, or title. Fields left out keep their values.",
        "inputSchema": {
            "type": "object",
            "properties": {"id": {"type": "integer"}, **TASK_FIELDS},
            "required": ["id"],
        },
    },
    {
        "name": "tasks_replace",
        "description": "Replaces the whole list. An empty list wipes it. Only when the user asks to start over or replace the list.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "tasks": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": TASK_FIELDS,
                        "required": ["title"],
                    },
                }
            },
            "required": ["tasks"],
        },
    },
    {
        "name": "tasks_decline",
        "description": "Records that the user doesn't want a task list in this session, so later additions are refused.",
        "inputSchema": {"type": "object", "properties": {}},
    },
]


def now():
    return datetime.datetime.now(datetime.timezone.utc).isoformat(timespec="milliseconds")


def log(kind, **fields):
    os.makedirs(RESULTS, exist_ok=True)
    with open(LOG, "a") as f:
        f.write(json.dumps({"at": now(), "pid": os.getpid(), "kind": kind, **fields}) + "\n")


def session_key():
    return os.environ.get("CLAUDE_CODE_HOST_SESSION_ID") or os.environ.get(
        "CLAUDE_CODE_SESSION_ID", "unknown"
    )


def list_path():
    return os.path.join(TASKS, session_key() + ".json")


def load():
    try:
        with open(list_path()) as f:
            return json.load(f)
    except (OSError, ValueError):
        return {"declined": False, "next_id": 1, "tasks": [], "replaced": None}


def save(data):
    os.makedirs(TASKS, exist_ok=True)
    tmp = list_path() + ".tmp"
    with open(tmp, "w") as f:
        json.dump(data, f, indent=2)
    os.replace(tmp, list_path())


def new_task(data, fields):
    task = {
        "id": data["next_id"],
        "title": fields["title"],
        "state": fields.get("state", "todo"),
        "status": fields.get("status", ""),
        "waiting_on": fields.get("waiting_on", ""),
        "updated": now(),
    }
    data["next_id"] += 1
    return task


def check_state(fields):
    state = fields.get("state")
    if state is not None and state not in STATES:
        raise ValueError(f"state must be one of {', '.join(STATES)}")


def call(name, args):
    data = load()
    if name == "tasks_get":
        return data
    if name == "tasks_decline":
        data["declined"] = True
        save(data)
        return "Recorded: no task list in this session."
    if name in ("task_add", "tasks_replace") and data["declined"]:
        raise ValueError("The user said not to keep a task list in this session.")
    if name == "task_add":
        check_state(args)
        task = new_task(data, args)
        data["tasks"].append(task)
        save(data)
        return task
    if name == "task_update":
        check_state(args)
        for task in data["tasks"]:
            if task["id"] == args["id"]:
                for key in ("title", "state", "status", "waiting_on"):
                    if key in args:
                        task[key] = args[key]
                task["updated"] = now()
                save(data)
                return task
        raise ValueError(f"No task with id {args['id']}.")
    if name == "tasks_replace":
        for fields in args["tasks"]:
            check_state(fields)
        data["replaced"] = {"at": now(), "tasks": data["tasks"]}
        data["tasks"] = [new_task(data, fields) for fields in args["tasks"]]
        save(data)
        return data
    raise ValueError(f"Unknown tool {name}.")


def reply(id_, result=None, error=None):
    message = {"jsonrpc": "2.0", "id": id_}
    if error is not None:
        message["error"] = error
    else:
        message["result"] = result
    sys.stdout.write(json.dumps(message) + "\n")
    sys.stdout.flush()


def main():
    log(
        "start",
        ppid=os.getppid(),
        # The folder's name only, since its full path holds the home folder.
        cwd=os.path.basename(os.getcwd()),
        env={k: os.environ.get(k) for k in LOGGED_ENV},
    )
    for line in sys.stdin:
        line = line.strip()
        if not line:
            continue
        try:
            message = json.loads(line)
        except ValueError:
            log("unparsable", line=line[:200])
            continue
        method = message.get("method")
        id_ = message.get("id")
        params = message.get("params") or {}
        if method == "tools/call":
            log("call", tool=params.get("name"), arguments=params.get("arguments"))
        else:
            log("message", method=method, params=params if method == "initialize" else None)
        if id_ is None:
            continue
        if method == "initialize":
            reply(
                id_,
                {
                    "protocolVersion": params.get("protocolVersion", "2025-06-18"),
                    "capabilities": {"tools": {}},
                    "serverInfo": {"name": "plates-tasks", "version": "0.0.1"},
                    "instructions": INSTRUCTIONS,
                },
            )
        elif method == "tools/list":
            reply(id_, {"tools": TOOLS})
        elif method == "tools/call":
            try:
                result = call(params.get("name"), params.get("arguments") or {})
                text = result if isinstance(result, str) else json.dumps(result, indent=2)
                reply(id_, {"content": [{"type": "text", "text": text}]})
            except (ValueError, KeyError) as e:
                log("call-error", error=str(e))
                reply(id_, {"content": [{"type": "text", "text": str(e)}], "isError": True})
        elif method == "ping":
            reply(id_, {})
        else:
            reply(id_, error={"code": -32601, "message": f"Method not found: {method}"})
    log("exit")


if __name__ == "__main__":
    main()
