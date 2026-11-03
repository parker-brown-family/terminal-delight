/* A stand-in `claude` for the War Room copy rig, compiled so that a copy of
 * it can be started the way the window starts a real one: from the agent's
 * own /proc/<pid>/exe. A shell script would show /bin/bash there.
 *
 * As an agent (no --fork-session): write a one-line transcript, hold it open
 * for life (the agent naming its own conversation), and run claude.sh beside
 * it for the screen, the response card and any asks.
 * As a copy (--fork-session): hand over to claude-copy.sh, which speaks
 * Claude's stream-json the way a copy does. */
#define _GNU_SOURCE
#include <ctype.h>
#include <fcntl.h>
#include <libgen.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/wait.h>
#include <unistd.h>

int main(int argc, char **argv) {
    char self[4096];
    ssize_t n = readlink("/proc/self/exe", self, sizeof self - 1);
    if (n < 0) return 1;
    self[n] = 0;
    char *dir = dirname(strdup(self));
    char script[4352];
    for (int i = 1; i < argc; i++) {
        if (strcmp(argv[i], "--fork-session") == 0) {
            snprintf(script, sizeof script, "%s/claude-copy.sh", dir);
            char **args = calloc(argc + 2, sizeof *args);
            args[0] = "bash";
            args[1] = script;
            for (int j = 1; j < argc; j++) args[j + 1] = argv[j];
            execv("/bin/bash", args);
            return 127;
        }
    }
    char id[64] = {0};
    FILE *u = fopen("/proc/sys/kernel/random/uuid", "r");
    if (!u || !fgets(id, sizeof id, u)) return 1;
    fclose(u);
    id[strcspn(id, "\n")] = 0;
    char cwd[2048];
    if (!getcwd(cwd, sizeof cwd)) return 1;
    char slug[2048];
    size_t len = strlen(cwd);
    for (size_t i = 0; i < len; i++) slug[i] = isalnum((unsigned char)cwd[i]) ? cwd[i] : '-';
    slug[len] = 0;
    char path[4352], cmd[4500];
    snprintf(path, sizeof path, "%s/.claude/projects/%s", getenv("HOME"), slug);
    snprintf(cmd, sizeof cmd, "mkdir -p '%s'", path);
    if (system(cmd) != 0) return 1;
    strncat(path, "/", sizeof path - strlen(path) - 1);
    strncat(path, id, sizeof path - strlen(path) - 1);
    strncat(path, ".jsonl", sizeof path - strlen(path) - 1);
    const char *size = getenv("SIZE") ? getenv("SIZE") : "100000";
    const char *model = getenv("MODEL") ? getenv("MODEL") : "claude-opus-5-5";
    FILE *t = fopen(path, "w");
    if (!t) return 1;
    fprintf(t,
            "{\"type\":\"assistant\",\"timestamp\":\"2026-10-22T19:00:00.000Z\",\"sessionId\":\"%s\","
            "\"cwd\":\"%s\",\"message\":{\"model\":\"%s\",\"role\":\"assistant\",\"usage\":{"
            "\"input_tokens\":2,\"cache_read_input_tokens\":%s,\"cache_creation_input_tokens\":0,"
            "\"output_tokens\":12},\"content\":[{\"type\":\"text\",\"text\":\"ok\"}]}}\n",
            id, cwd, model, size);
    fclose(t);
    int held = open(path, O_RDONLY);
    if (held < 0) return 1;
    setenv("RIG_SESSION", id, 1);
    pid_t c = fork();
    if (c == 0) {
        snprintf(script, sizeof script, "%s/claude.sh", dir);
        execl("/bin/bash", "bash", script, (char *)0);
        _exit(127);
    }
    int st;
    waitpid(c, &st, 0);
    for (;;) pause();
}
