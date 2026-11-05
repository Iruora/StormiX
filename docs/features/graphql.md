# GraphQL

A step can send a GraphQL query with typed variables. Stormix sends it as JSON with `Content-Type: application/json`, and fails the step when the response carries GraphQL `errors`, even with status 200.

<Shot name="graphql-steps" alt="A GraphQL step in the Steps tab" />

## In the app

Pick **graphql** as the step's body in the Steps tab. It has fields for the **query**, the **variables**, the **operation name** and **pass even with GraphQL errors**. Switching a json or raw body that holds `{ "query": ... }` to graphql converts it. The YAML editor completes the `graphql` keys.

## In a test file

```yaml
- name: Read a user
  request:
    method: POST
    url: "{{baseUrl}}/graphql"
    body:
      graphql:
        query: |
          query User($id: Int!) { user(id: $id) { id name } }
        variables:
          id: "{{user_id}}"
        operation_name: User
```

## Variables take the type the query declares

GraphQL servers reject `"7"` for an `Int!`. A templated variable takes the type its query declares, so `id: "{{user_id}}"` goes out as the number `7` for `$id: Int!`, and as a string for `$id: ID!`.

Variables can also be JSON text with bare `{{variables}}` in it, like `{ "id": {{userId}} }`, as Postman and Bruno write them. The variables are filled in before the text is parsed.

## Errors

A GraphQL step gets a **no GraphQL errors** check, which fails with the first error's message when the response has `errors`. It also fails on a 4xx or 5xx, or a body that is not JSON. Load tests count these as errors. Set `allow_errors: true` on a step that expects errors.

<Shot name="graphql-errors" alt="A run with a GraphQL error reported" />

`stormix validate` and the editor report an empty query and variables that are not a JSON object, and warn about undefined variables. Full details: [GraphQL in the test file reference](/reference/test-file#graphql).
