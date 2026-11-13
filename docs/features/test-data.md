# Test data

A load test where every iteration asks for the same user only measures that user, often straight from a cache. **Test data** gives the load test a list of values, such as user IDs, and each iteration (or each virtual user) picks one, so the load spreads over many users, IDs or products.

## Setting it up

In the app, open **Load test** and find **Test data** in the setup dialog:

- **Add a list** and type the variable name on the first line, then one value per line. Use the variable as `{{userId}}` in any step.
- **Import CSV…** reads a CSV file whose first line names the variables, such as `userId,password`. Up to 1,000 rows are copied into the test file; a bigger file is linked and read when the test starts.
- **Pick**: **Random**, **In order** or **Each once**.
- **New row for**: **Each iteration** or **Each user**.

<Shot name="test-data-setup" alt="Test data in the load test setup: a list of user IDs, each virtual user keeping one" />

The lists are saved into the test file's `data` section:

```yaml
flow:
  name: Users
  steps:
    - name: Get a user
      request: { method: GET, url: "{{base_url}}/user/{{userId}}" }
load:
  virtual_users: { count: 5, duration: 1m }
data:
  - variable: userId
    values: [user-1, user-2]
    pick: sequential
    per: user
```

Five users, two IDs, in order, one per user: users 1, 3 and 5 send `user-1` all through the test, and users 2 and 4 send `user-2`.

## Where the values come from

| Form | YAML | Fills |
|---|---|---|
| One variable | `variable: userId` and `values: [1, 2, 3]` | `{{userId}}` |
| Several variables that go together | `rows: [{ user: ada, password: x1 }, { user: grace, password: x2 }]` | `{{user}}` and `{{password}}` from the same row |
| A CSV file | `file: users.csv` (relative to the test file) | One variable per column, named by the first line |

A CSV file may quote fields (`"Lovelace, Ada"`), with `""` for a quote inside one.

## How rows are picked

| `pick` | Each time a row is needed |
|---|---|
| `random` (default) | Any row, at random. Rows repeat, and over many iterations each gets about the same share. |
| `sequential` | The next row, starting over after the last. |
| `unique` | The next row, each used once. When every row has been used, no new iteration starts and the test ends early. Running iterations finish, and the thresholds still decide whether it passed. |

| `per` | A new row for |
|---|---|
| `iteration` (default) | Every run of the flow. |
| `user` | Every virtual user, when it starts. The user keeps that row for all its iterations, like a real user who stays logged in as themselves. With `pick: unique`, users beyond the last row don't start. Only `virtual_users` has lasting users: under the other profiles every iteration is a new user, so each iteration picks. |

Several lists pick independently. Values from test data win over the flow's `variables` and the environment, and steps that extract a variable of the same name still replace it for the steps after them.

A **run once** (the app's **Run**, `stormix run`) and **Diagnose** use the first row of each list.

## In the results

The results, the HTML report and `stormix load` show each list: how many rows were picked and, for lists of up to 50 rows, how often each row was, so you can see the load was spread as planned.

<Shot name="test-data-result" alt="A load test result showing how often each user ID was picked" />
