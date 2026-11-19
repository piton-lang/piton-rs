# Test Settings

## Description

Each test is judged against what `--against` says, else what the test says, else the project's test settings, else both. The agent and the number of jobs come from the flag, else the test settings, else claude and 4. The model comes from the flag, else the test settings, else the tool's own default. The settings are described in the testing specification.
