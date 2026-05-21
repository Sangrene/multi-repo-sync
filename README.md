# Multi-repo-sync
This CLI tool is used to manage pull requests creation, merges and releases hapening across multiple repositories.

## Usage
Create the JSON configuration file, matching this example :
```json
{
  "pat": "MY_TOKEN",
  "pattern": {
    "title": "Main to production",
    "body": "Main staging to production"
  },
  "repositories": [
    {
      "owner": "Sangrene",
      "repo": "multi-repo-sync",
      "origin": "main",
      "target": "release"
    }
  ]
}
```
The `pattern` describes the PR title and body. 

Run the script.
In debug 
```cargo run --config=trueconfig.json --tag=v1.23.2```

ie
```.multi-repo-sync --config=config.json --tag=v1.23.2```