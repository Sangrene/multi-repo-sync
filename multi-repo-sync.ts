import { Octokit } from "npm:@octokit/rest";
import * as path from "https://deno.land/std/path/mod.ts";
import { parseArgs } from "jsr:@std/cli/parse-args";

const flags = parseArgs(Deno.args, {
  string: ["config", "release"],
});

interface Repo {
  owner: string;
  repo: string;
  origin: string;
  target: string;
  wait?: number;
}

interface Config {
  pat: string;
  repositories: Repo[];
  pattern: {
    title: string;
    body: string;
  };
}

const getFlags = (): {
  config?: string;
  release?: string;
} => {
  return {
    config: flags.config,
    release: flags.release,
  };
};

const getConfig = async (): Promise<{ config: Config }> => {
  const file =
    getFlags().config ||
    path.fromFileUrl(
      import.meta.url.replace("multi-repo-sync.ts", "config.json")
    );

  return { config: JSON.parse(await Deno.readTextFile(file)) as Config };
};

const login = async ({ config }: { config: Config }) => {
  const octokit = new Octokit({ auth: config.pat });
  const user = await octokit.rest.users.getAuthenticated();

  return { octokit, user, config };
};

type Github = Awaited<ReturnType<typeof login>>["octokit"];
type User = Awaited<ReturnType<typeof login>>["user"];
type Branch = Awaited<
  ReturnType<Github["rest"]["repos"]["listBranches"]>
>["data"][number];

const BRANCHES_PER_PAGE = 100;

const guardRepoBranches = async (
  { owner, repo, origin, target }: Repo,
  github: Github
) => {
  const errors = [];
  let base: Branch | undefined = undefined;
  let head: Branch | undefined = undefined;
  let page = 0;
  while (!head && !base && errors.length === 0) {
    const branches = (
      await github.rest.repos.listBranches({ repo, owner, per_page: BRANCHES_PER_PAGE, page })
    ).data;
    if (!base) {
      base = branches.find((branch) => branch.name === origin);
    }

    if (!head) {
      head = branches.find((branch) => branch.name === target);
    }

    if (head && base) {
      break;
    }

    if (branches.length === BRANCHES_PER_PAGE) {
      page++;
    }

    if (branches.length < BRANCHES_PER_PAGE) {
      break;
    }
  }

  if (!base) {
    errors.push(
      new Error(
        `Could not find BASE branch for repo: ${repo}, branch: ${origin}`
      )
    );
  }

  if (!head) {
    errors.push(
      new Error(
        `Could not find BASE branch for repo: ${repo}, branch: ${target}`
      )
    );
  }

  return errors.length === 0 ? { base, head } : { errors };
};

const createPullRequests = async ({
  config,
  octokit,
}: {
  config: Config;
  octokit: Github;
}) => {
  return {
    config,
    octokit,
    pullRequests: await Promise.all(
      config.repositories.map(async (r) => {
        const { origin, owner, repo, target, wait } = r;
        if (wait) {
          await new Promise((resolve) => {
            setTimeout(resolve, wait * 1000);
          });
        }
        const { errors } = await guardRepoBranches(r, octokit);
        if (errors)
          return {
            repo,
            owner,
            errors,
          };

        try {
          const pr = await octokit.rest.pulls.create({
            owner,
            repo,
            base: target,
            head: origin,
            title: config.pattern.title,
            body: config.pattern.body,
          });
          console.log(
            `Created PR ${pr.data.number} on repo ${repo} : ${origin} TO ${target}`
          );
          return {
            result: { ...pr },
            owner,
            repo,
          };
        } catch (e) {
          if (e.status === 422) {
            return {
              owner,
              repo,
              errors: e.response.data.errors.map(
                (err: any) => new Error(`${err.message} on repo ${repo}`)
              ) as Error[],
            };
          }

          return {
            owner,
            repo,
            errors: [e],
          };
        }
      })
    ),
  };
};

const mergePullRequests = async ({
  pullRequests,
  config,
  octokit,
}: Awaited<ReturnType<typeof createPullRequests>>) => {
  const merges = await Promise.all(
    pullRequests.map(async (pr) => {
      if (pr.errors) {
        return {
          errors: pr.errors,
          repo: pr.repo,
          owner: pr.owner,
        };
      }
      try {
        const result = await octokit.rest.pulls.merge({
          owner: pr.owner,
          repo: pr.repo,
          pull_number: pr.result.data.number,
        });
        console.log(`Merged PR ${pr.result.data.number} on repo ${pr.repo}`);
        return {
          result,
          owner: pr.owner,
          repo: pr.repo,
        };
      } catch (e) {
        return { errors: [e], owner: pr.owner, repo: pr.repo, config };
      }
    })
  );

  return {
    config,
    octokit,
    pullRequests,
    merges,
  };
};

const createReleases = async ({
  config,
  merges,
  octokit,
  pullRequests,
}: Awaited<ReturnType<typeof mergePullRequests>>) => {
  const releaseName = getFlags().release;

  return {
    config,
    octokit,
    pullRequests,
    merges,
    releases: releaseName
      ? await Promise.all(
          merges.map(async (merge) => {
            if (merge.errors) {
              return {
                owner: merge.owner,
                repo: merge.repo,
                errors: merge.errors,
              };
            }
            const result = await octokit.rest.repos.createRelease({
              owner: merge.owner,
              repo: merge.repo,
              tag_name: releaseName,
              generate_release_notes: true,
              make_latest: "true",
              target_commitish: merge.result?.data.sha,
            });
            return {
              owner: merge.owner,
              repo: merge.repo,
              result,
            };
          })
        )
      : undefined,
  };
};

const results = await getConfig()
  .then(login)
  .then(createPullRequests)
  .then((res) => {
    console.table(
      res.pullRequests.map((pr) => ({
        repo: pr.repo,
        owner: pr.owner,
        status: pr.errors ? "❌" : "✅",
        result: pr.result
          ? `${pr.result?.data.title} ${pr.result.data.html_url}`
          : "No result",
        errors: pr.errors?.map((e) => e.message),
      }))
    );
    return res;
  })
  .then(mergePullRequests)
  .then((res) => {
    console.table(
      res.merges.map((pr) => ({
        repo: pr.repo,
        owner: pr.owner,
        status: pr.errors ? "❌" : "✅",
        result: pr.result
          ? `${pr.result?.data.message} ${pr.result?.data.sha}`
          : "No result",
        errors: pr.errors?.map((e) => e.message),
      }))
    );
    return res;
  })
  .then(createReleases)
  .then((res) => {
    console.table(
      res.releases?.map((rel) => ({
        repo: rel.repo,
        owner: rel.owner,
        status: rel.errors ? "❌" : "✅",
        result: rel.result?.data.html_url,
        errors: rel.errors?.map((e) => e.message),
      }))
    );
  });
