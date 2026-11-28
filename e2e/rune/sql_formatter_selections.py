"""Refuse incomplete selected fragments without contaminating neighboring SQL."""
from offline import key
from scenarios import query_steps, tap, visible
from sql_formatter import draft, selection


class SelectionRepairJourney:
    def test_partial_statement_selections_and_neighboring_format(self):
        # An editor can contain unfinished statements. Each pair selects
        # only the newly pasted line: first an unacceptable fragment, then
        # its completed SQL. The complete document is the observable ledger
        # of which ranges the native toolbar changed, not a direct API call.
        pairs = [
            ("select id from customers limit 2,5,8;", "select id from customers limit 2,5;",
             "SELECT\n  id\nFROM\n  customers\nLIMIT\n  2, 5;"),
            ("select customers.;", "select customers.id from customers;",
             "SELECT\n  customers.id\nFROM\n  customers;"),
            ("pragma main.user_version(extra, value);", "pragma main.user_version;",
             "PRAGMA main.user_version;"),
            ("select id from customers union all id;", "select id from customers union all select 0;",
             "SELECT\n  id\nFROM\n  customers\nUNION ALL\nSELECT\n  0;"),
            ("select case when id=1 then 'one' else 'other' from customers;",
             "select case when id=1 then 'one' else 'other' end from customers;",
             "SELECT\n  CASE\n    WHEN id = 1 THEN 'one'\n    ELSE 'other'\n  END\nFROM\n  customers;"),
            ("select @# from customers;", "select @name from customers;",
             "SELECT\n  @name\nFROM\n  customers;"),
        ]
        baseline = self._formatter_fixture({})
        route = [*query_steps("")[:4]]
        result = []
        t = 4.9
        for failed, repaired, expected in pairs:
            for source, target in [(failed, failed), (repaired, expected)]:
                # Include a visibly unformatted prefix in each failed range.
                # A partial rewrite cannot look like a correct refusal.
                if source == failed:
                    source = "select  0; " + source
                    target = source
                if result:
                    route.append(key(t, "End", cmd=True))
                    t += .20
                route.extend([{"t": t, "type": "paste", "text": ("\n" if result else "") + source},
                              {**key(t + .25, "Home"), "shift": True},
                              tap(t + .55, "sql-format")])
                result.append(target)
                t += 1.5
        rendered = self.replay(route, until=t + .6, timeout=140)
        self.assertEqual(draft(rendered), "\n".join(result))
        first = "\n".join(result[:-1]).count("\n") + 1
        self.assertEqual(selection(rendered), list(enumerate(result[-1].splitlines(), first)))
        self.assertFalse(visible(rendered, key="query-notice"))
        restored = self.replay([*route, key(t + .65, "z", cmd=True)], until=t + 1.25, timeout=140)
        self.assertEqual(draft(restored), "\n".join([*result[:-1], pairs[-1][1]]))
        self._no_query_or_settings_write((rendered, restored), baseline)
