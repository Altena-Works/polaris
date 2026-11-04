//! Tests of the turn-only memory's screen, the verdicts it leaves and its gate.
//!
//! The screen takes each message whole before it is cut into chunks and leaves
//! a verdict on the chunks; the gate stands in front of everything that leaves
//! the memory or is quoted.

use super::*;

// Fakes. KEY straddles the first cut of the tool result that holds it,
// and its second half is made of words, so a screen that saw the pieces
// apart would miss it.
const PASSWORD: &str = "hunter2hunter2";
const TOKEN: &str = "tok_ABCdef0123456789";
const GITHUB: &str = "ghp_abcdefghijklmnopqrstuvwxyz0123456789";
const KEY: &str = "sk-alpha-bravo-charlie-delta-echo-foxtrot-golf-hotel-india";
const LEAK: &str = "ghp_leakyleakyleakyleakyleakyleakyleaky";
/// What must be nowhere: the fakes whole, and the pieces a leak would show.
const SECRETS: &[&str] = &[
    PASSWORD,
    TOKEN,
    GITHUB,
    "abcdefghijklmnopqrstuvwxyz",
    KEY,
    "charlie-delta",
    "foxtrot",
    LEAK,
    "leakyleaky",
];

/// What a fixture writer says to a request: a line, or nothing.
type Answer = Box<dyn Fn(&str) -> Option<String> + Send + Sync>;

/// A writer that keeps what it is sent and answers each request through
/// `answer` (an error where that gives nothing). It claims `boundary`.
struct Recording {
    boundary: polaris_provider::NetworkBoundary,
    answer: Answer,
    sent: std::sync::Mutex<Vec<String>>,
}

impl Recording {
    fn new(
        boundary: polaris_provider::NetworkBoundary,
        answer: impl Fn(&str) -> Option<String> + Send + Sync + 'static,
    ) -> Arc<Self> {
        Arc::new(Self {
            boundary,
            answer: Box::new(answer),
            sent: Default::default(),
        })
    }

    fn sent(&self) -> Vec<String> {
        self.sent.lock().expect("lock").clone()
    }
}

#[async_trait::async_trait]
impl polaris_provider::Provider for Recording {
    fn network_boundary(&self) -> polaris_provider::NetworkBoundary {
        self.boundary
    }

    async fn complete(
        &self,
        request: polaris_provider::CompletionRequest,
    ) -> Result<polaris_provider::CompletionResponse, polaris_provider::ProviderError> {
        let asked = request
            .messages
            .iter()
            .map(|m| m.content.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        self.sent.lock().expect("lock").push(asked.clone());
        (self.answer)(&asked)
            .map(|text| polaris_provider::CompletionResponse {
                text,
                ..Default::default()
            })
            .ok_or_else(|| {
                polaris_provider::ProviderError::Http("the fixture writer has no answer".into())
            })
    }
}

/// A decider that says yes to everything and keeps what it was asked.
#[derive(Default)]
struct Watching(std::sync::Mutex<Vec<String>>);

impl Watching {
    fn asked(&self) -> Vec<String> {
        self.0.lock().expect("lock").clone()
    }
}

#[async_trait::async_trait]
impl Decider for Watching {
    async fn decide(
        &self,
        state: &serde_json::Value,
        questions: &BTreeMap<String, Question>,
    ) -> Result<polaris_provider::decision::Decision, polaris_provider::decision::DecisionError>
    {
        let mut asked = self.0.lock().expect("lock");
        asked.push(state.to_string());
        let answers = questions
            .iter()
            .map(|(key, question)| {
                let Question::Noul { instructions } = question else {
                    unreachable!("only yes/no questions are asked")
                };
                asked.push(instructions.clone());
                let answer = polaris_provider::decision::Answer {
                    noul: Some(0.9),
                    ..Default::default()
                };
                (key.clone(), answer)
            })
            .collect();
        Ok(polaris_provider::decision::Decision {
            answers,
            input_tokens: None,
            output_tokens: None,
        })
    }
}

/// An embedder that keeps every text it is given.
struct Spy {
    model: crate::conversation_memory::EmbeddingModel,
    texts: std::sync::Mutex<Vec<String>>,
}

impl Spy {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            model: crate::conversation_memory::EmbeddingModel {
                model: "fixture".into(),
                revision: "1".into(),
                dimension: 2,
            },
            texts: Default::default(),
        })
    }

    fn texts(&self) -> Vec<String> {
        self.texts.lock().expect("lock").clone()
    }
}

impl StrictEmbedder for Spy {
    fn metadata(&self) -> &crate::conversation_memory::EmbeddingModel {
        &self.model
    }

    fn embed_passage<'a>(
        &'a self,
        text: &'a str,
    ) -> crate::conversation_memory::StrictFuture<'a, Vec<f32>> {
        self.texts.lock().expect("lock").push(text.to_owned());
        Box::pin(async { Ok(vec![1.0, 0.0]) })
    }

    fn embed_query<'a>(
        &'a self,
        text: &'a str,
    ) -> crate::conversation_memory::StrictFuture<'a, Vec<Vec<f32>>> {
        self.texts.lock().expect("lock").push(text.to_owned());
        Box::pin(async { Ok(vec![vec![1.0, 0.0]]) })
    }
}

/// A tool result with `KEY` starting at its 381st character, so that the
/// first cut, at 400, falls inside the key.
fn build_output() -> String {
    let mut filler = String::new();
    while filler.len() < 380 {
        filler.push_str("build step ok ");
    }
    filler.truncate(379);
    format!("{filler} {KEY} then the build finished fine")
}

fn tool_call(id: &str, name: &str, arguments: serde_json::Value) -> ToolCall {
    ToolCall {
        id: id.into(),
        name: name.into(),
        arguments,
    }
}

fn bash(id: &str, command: &str) -> Message {
    Message::assistant_with_tool_calls(
        String::new(),
        vec![tool_call(
            id,
            "bash",
            serde_json::json!({ "command": command }),
        )],
    )
}

/// A turn in which the model made one call (answered `result`) and then
/// said `answer`, and the next input.
fn ran(input: &str, call: Message, result: &str, answer: &str) -> Vec<Message> {
    let id = call.tool_calls[0].id.clone();
    vec![
        Message::user(input),
        call,
        Message::tool_result(id, result),
        Message::assistant(answer),
        Message::user("now"),
    ]
}

/// The chunks of tool origin, as their text and the screen's verdict.
fn tool_chunks(memory: &TurnMemory) -> Vec<(&str, Verdict)> {
    memory
        .log
        .iter()
        .filter(|chunk| chunk.origin == Origin::Tool)
        .map(|chunk| (chunk.text.as_str(), chunk.verdict))
        .collect()
}

#[tokio::test]
async fn a_key_across_the_first_cut_is_redacted_whole_before_the_message_is_cut() {
    let output = build_output();
    // Premise: cut as it came, the key would lie in two pieces.
    let first = pieces(&output)[0].1;
    assert!(
        first.contains("sk-alpha") && !first.contains("charlie"),
        "{first}"
    );

    let mut memory = TurnMemory::default();
    let call = bash("c1", "tail build.log");
    memory
        .absorb(&ran("Show me the build log.", call, &output, "Done."), None)
        .await;
    let texts: Vec<&str> = memory.log.iter().map(|c| c.text.as_str()).collect();
    for fragment in ["sk-alpha", "alpha-bravo", "charlie", "foxtrot", "india"] {
        assert!(
            texts.iter().all(|t| !t.contains(fragment)),
            "{fragment}: {texts:?}"
        );
    }
    assert!(
        texts.iter().any(|t| t.contains("[REDACTED] then the")),
        "{texts:?}"
    );
    assert_eq!(
        tool_chunks(&memory)
            .iter()
            .map(|(_, v)| *v)
            .collect::<Vec<_>>(),
        [Verdict::Redacted; 2],
        "both pieces of the tool result carry the verdict"
    );
}

#[tokio::test]
async fn no_secret_reaches_a_writer_the_decider_the_embedder_a_quote_or_the_stores() {
    use polaris_provider::NetworkBoundary::Unknown;
    // A writer that repeats what it should not know, in a line and in a
    // decision.
    let writer = Recording::new(Unknown, |request| {
        Some(if request.contains("retention_days") {
            format!("topic=retention; chose=retention_days=37 {LEAK}")
        } else {
            format!("topic=check; did=looked at things {LEAK}")
        })
    });
    let decider = Watching::default();
    let spy = Spy::new();
    let mut memory = factory(false, Some(writer.clone()), Some(spy.clone()))();

    let mut messages = vec![
        // 1: the person pastes secrets (YAML, JSON, a token) into the
        // first line, next to a path the decider will be asked about.
        Message::user(format!(
            "Use `settings/app.yaml` with password: {PASSWORD} and {{\"access_token\": \"{TOKEN}\"}} and {GITHUB}, keep them out of the repo.\nThanks."
        )),
        Message::assistant("Understood, I will keep them out."),
        // 2: a tool result with a key across the first cut.
        Message::user("Show me the build log."),
        bash("c2", "tail build.log"),
        Message::tool_result("c2", build_output()),
        Message::assistant("The build finished."),
        // 3: a config file, in YAML and JSON.
        Message::user("And the config?"),
        bash("c3", "cat config.yml"),
        Message::tool_result(
            "c3",
            format!(
                "database:\n  password: {PASSWORD}\n  user: app\n{{\"access_token\": \"{TOKEN}\"}}\n"
            ),
        ),
        Message::assistant("It is a database section."),
        // 4 and 5: a denied path, through bash and through read.
        Message::user("Check the env."),
        bash("c4", "cat .env"),
        Message::tool_result("c4", format!("DB_PASSWORD={PASSWORD}\nSECRET_NOTE=plain")),
        Message::assistant("Looked."),
        Message::user("Read it again."),
        Message::assistant_with_tool_calls(
            String::new(),
            vec![tool_call(
                "c5",
                "read",
                serde_json::json!({ "path": ".env" }),
            )],
        ),
        Message::tool_result("c5", format!("DB_PASSWORD={PASSWORD}")),
        Message::assistant("Read."),
    ];
    // Eight turns more, one a decision, so that lines are evicted and
    // folded and a fact is made.
    for i in 6..=13 {
        let (input, answer) = if i == 7 {
            (
                "Pin retention_days=37.".to_owned(),
                "Pinned retention_days=37.".to_owned(),
            )
        } else {
            (format!("question {i}"), format!("answer {i}"))
        };
        messages.push(Message::user(input));
        messages.push(Message::assistant(answer));
    }
    messages.push(Message::user("now"));
    memory.absorb(&messages, Some(&decider)).await;

    // The channels were used, so that what follows is not vacuous.
    assert_eq!(memory.turns, 13);
    assert!(writer.sent().len() >= 12, "{}", writer.sent().len());
    assert!(!decider.asked().is_empty());
    assert!(spy.texts().len() > 10);
    assert!(memory.facts.contains_key("retention"));
    assert!(memory.dictionary.contains_key("settings/app.yaml"));
    assert!(memory.log.iter().any(|c| c.text == WITHHELD_PATH));

    // What left.
    let mut left = writer.sent();
    left.extend(decider.asked());
    left.extend(spy.texts());
    // What is held.
    let mut held: Vec<String> = memory.log.iter().map(|c| c.text.clone()).collect();
    held.extend(memory.ledger.iter().map(|(_, line)| line.clone()));
    held.extend(memory.dictionary.iter().flat_map(|(term, seen)| {
        std::iter::once(term.clone()).chain(seen.iter().map(|(_, line)| line.clone()))
    }));
    held.extend(memory.facts.values().map(|fact| fact.text.clone()));
    held.push(memory.direction.clone());
    // What is quoted.
    let mut quoted = Vec::new();
    for query in [
        "What did the build log show?",
        "the config password database",
        "env file read again",
        "settings app.yaml keep out of the repo",
        "alpha bravo charlie delta foxtrot",
    ] {
        quoted.extend(memory.recall(query).await.map(|m| m.content));
    }
    assert!(
        quoted.iter().any(|q| q.contains("fact (turn 7)")),
        "{quoted:?}"
    );
    for (what, texts) in [("left", &left), ("held", &held), ("quoted", &quoted)] {
        for text in texts {
            for secret in SECRETS {
                assert!(
                    !text.contains(secret),
                    "{secret} is in what was {what}: {text}"
                );
            }
        }
    }
}

#[tokio::test]
async fn what_the_memory_holds_is_gated_again_when_it_is_quoted_and_as_it_enters() {
    // Held as if an earlier version had let it in.
    let mut memory = TurnMemory {
        direction: format!("topic=x; did=used {LEAK}"),
        ..Default::default()
    };
    memory.facts.insert(
        "t".into(),
        Fact {
            turn: 1,
            text: format!("port=80 {LEAK}"),
            cites: 0..1,
        },
    );
    memory
        .ledger
        .push_back((1, format!("asked=go; at=x {LEAK}")));
    memory
        .dictionary
        .insert("term".into(), vec![(1, format!("a line with {LEAK}"))]);
    memory.log.push_back(Chunk {
        id: 0,
        turn: 1,
        offset: Some(0),
        text: format!("a chunk with term {LEAK}"),
        vector: None,
        origin: Origin::User,
        verdict: Verdict::Keep,
    });
    memory.next_chunk = 1;
    let quote = memory.recall("term chunk").await.expect("quoted").content;
    assert!(!quote.contains("leaky"), "{quote}");
    for start in [
        "fact (turn 1",
        "direction: ",
        "T1: ",
        "- `term` (turn 1)",
        "- [c0 turn 1 @0]",
    ] {
        assert!(
            quote
                .lines()
                .any(|l| l.starts_with(start) && l.contains("[REDACTED]")),
            "{start}: {quote}"
        );
    }

    // A decision enters the facts through the gate too.
    memory.settle(
        2,
        &format!("topic=retention; chose=retention_days=37 {LEAK}"),
        "retention_days=37",
        "retention_days=37",
        1..2,
    );
    assert_eq!(
        memory.facts["retention"].text,
        "retention_days=37 [REDACTED]"
    );
}

#[test]
fn the_finished_line_cites_its_chunks_and_the_files_it_may_name_and_leaves_through_the_gate() {
    let text = "Look at config/.env, src/app.rs and config/app.toml.";
    assert_eq!(
        cited(format!("topic=x; did=used {LEAK}"), &(3..5), text),
        "topic=x; did=used [REDACTED]; src=c3-c4; at=config/app.toml|src/app.rs"
    );
    assert_eq!(cited("asked=a".into(), &(7..8), ""), "asked=a; src=c7");
    assert_eq!(cited("asked=a".into(), &(0..0), "no paths here"), "asked=a");
    // A path runs to 200 characters at most.
    let long = format!("{}x", "a/".repeat(150));
    let line = cited("asked=a".into(), &(0..1), &long);
    assert_eq!(
        field(&line, "at").expect("names the file").chars().count(),
        200
    );
    // Nothing but a secret is left as the mark that something was there.
    assert_eq!(cited(GITHUB.into(), &(0..0), ""), WITHHELD);
}

#[tokio::test]
async fn what_is_sent_to_the_writer_and_what_it_writes_both_pass_the_gate() {
    use polaris_provider::NetworkBoundary::Unknown;
    let writer = Recording::new(Unknown, |_| Some(format!("topic=check; did=used {LEAK}")));
    // A request that holds a secret, as no turn's would, and an answer
    // that repeats one.
    let request = format!("USER: use {GITHUB} now\nASSISTANT: ok");
    let line = write(Some(&*writer), LINE_SYSTEM, &request, LINE_TOKENS).await;
    assert_eq!(line.as_deref(), Some("topic=check; did=used [REDACTED]"));
    assert_eq!(writer.sent(), ["USER: use [REDACTED] now\nASSISTANT: ok"]);
    // With nothing but a secret to send, the writer is not asked.
    assert_eq!(
        write(Some(&*writer), LINE_SYSTEM, GITHUB, LINE_TOKENS).await,
        None
    );
    assert_eq!(writer.sent().len(), 1);
}

#[tokio::test]
async fn the_decider_is_asked_only_through_the_gate() {
    let line = format!("asked=use `settings/app.yaml` with {LEAK}");
    let mut found = candidates(&line);
    assert!(!found.is_empty());
    // A term that carries a secret, which `candidates` would not hand over.
    found.push((GITHUB.to_owned(), Kind::Identifier));
    let decider = Watching::default();
    let specific = judge(&found, &line, Some(&decider)).await;
    // It is not asked about, so it is not judged specific either.
    assert_eq!(specific, ["settings/app.yaml"]);
    let asked = decider.asked();
    assert_eq!(asked.len(), 2, "{asked:?}");
    assert!(
        asked
            .iter()
            .all(|a| !a.contains("leaky") && !a.contains("ghp_")),
        "{asked:?}"
    );
    assert!(asked[0].contains("settings/app.yaml") && asked[0].contains("[REDACTED]"));
    // With nothing but a secret to show it, no question is asked at all.
    let before = decider.asked().len();
    judge(&found, GITHUB, Some(&decider)).await;
    assert_eq!(decider.asked().len(), before);
}

#[tokio::test]
async fn the_embedder_is_given_only_what_the_screen_left_of_a_chunk_and_of_a_query() {
    let spy = Spy::new();
    let mut memory = factory(false, None, Some(spy.clone()))();
    let call = bash("c1", "cat config.yml");
    let turn = ran(
        &format!("Use password: {PASSWORD} here"),
        call,
        &format!("token: {TOKEN}"),
        "Done.",
    );
    memory.absorb(&turn, None).await;
    memory
        .recall(&format!("what about password: {PASSWORD} please"))
        .await;
    let texts = spy.texts();
    assert!(texts.len() >= 4, "{texts:?}");
    assert!(
        texts
            .iter()
            .all(|t| !t.contains(PASSWORD) && !t.contains(TOKEN)),
        "{texts:?}"
    );
    assert!(
        texts.iter().any(|t| t.contains("password: [REDACTED]")),
        "{texts:?}"
    );
}

#[test]
fn a_term_the_memory_screen_catches_is_not_a_dictionary_candidate() {
    // None of these meets the older secret-shape rule; each is caught by
    // the memory's screen.
    let found: Vec<String> = candidates(
        "Keys `AKIAIOSFODNN7EXAMPLE` and `Zq8Lm2Xp4Rt6Vb1Nc3Hd5Jf7Gs9Kw0Aa` and `api_key: abcd1234efgh`, but `PlanBuilder` stays.",
    )
    .into_iter()
    .map(|(term, _)| term)
    .collect();
    assert!(found.contains(&"PlanBuilder".to_owned()), "{found:?}");
    // The name of a key is a fair term; its value, and the others, are not.
    assert!(found.contains(&"api_key".to_owned()), "{found:?}");
    for secret in ["AKIA", "Zq8Lm2", "abcd1234"] {
        assert!(
            found.iter().all(|term| !term.contains(secret)),
            "{secret}: {found:?}"
        );
    }
    assert_eq!(found.len(), 2, "{found:?}");
}

#[tokio::test]
async fn names_the_person_may_ask_about_survive_the_screen_into_the_quote() {
    // The shared screen takes all three for secrets.
    let names = [
        "RewindBoundaryCompletionV1",
        "Sources/PolarisSettings/BootstrapV2Publisher",
        "max_tokens=256",
    ];
    let messages = vec![
        Message::user(
            "Is RewindBoundaryCompletionV1 still read from Sources/PolarisSettings/BootstrapV2Publisher with max_tokens=256?",
        ),
        Message::assistant(
            "Yes: RewindBoundaryCompletionV1 comes from Sources/PolarisSettings/BootstrapV2Publisher, max_tokens=256.",
        ),
        Message::user("now"),
    ];
    let mut memory = TurnMemory::default();
    memory.absorb(&messages, None).await;
    let quote = memory
        .recall("RewindBoundaryCompletionV1 BootstrapV2Publisher max_tokens")
        .await
        .expect("quoted")
        .content;
    for name in names {
        assert!(quote.contains(name), "{name} in {quote}");
        assert!(
            memory.log.iter().any(|chunk| chunk.text.contains(name)),
            "{name} in the log"
        );
    }
    assert!(
        memory
            .log
            .iter()
            .all(|chunk| chunk.verdict == Verdict::Keep)
    );
}

#[tokio::test]
async fn the_writer_is_called_for_a_turn_the_screen_left_alone_and_for_no_other() {
    use polaris_provider::NetworkBoundary::{LocalOnly, Unknown};
    // The setting, the writer's boundary, the input, the answer, a tool
    // result if any, whether the writer is asked, and how the line starts.
    let rows = [
        (
            true,
            LocalOnly,
            "first",
            "answer",
            None,
            true,
            "topic=written; src=",
        ),
        (
            true,
            Unknown,
            "first",
            "answer",
            None,
            false,
            "asked=first; answered=answer; src=",
        ),
        (
            false,
            Unknown,
            "first",
            "answer",
            None,
            true,
            "topic=written; src=",
        ),
        (
            false,
            Unknown,
            "set API_TOKEN=abc123xyz789",
            "answer",
            None,
            false,
            "asked=set API_TOKEN=[REDACTED]; answered=answer; src=",
        ),
        (
            false,
            Unknown,
            "first",
            "the key is ghp_abcdefghijklmnopqrstuvwxyz0123456789 ok",
            None,
            false,
            "asked=first; answered=the key is [REDACTED] ok; src=",
        ),
        (
            false,
            Unknown,
            GITHUB,
            "answer",
            None,
            false,
            "asked=withheld; answered=answer; src=",
        ),
        // What a tool printed is never sent, and does not stop the writer.
        (
            false,
            Unknown,
            "first",
            "answer",
            Some("API_TOKEN=abc123xyz789"),
            true,
            "topic=written; src=",
        ),
    ];
    for (local_only, boundary, input, answer, result, asked, line) in rows {
        let writer = Recording::new(boundary, |_| Some("topic=written".to_owned()));
        let mut memory = factory(local_only, Some(writer.clone()), None)();
        let mut turn = vec![Message::user(input)];
        if let Some(result) = result {
            turn.push(bash("c1", "cat build.cfg"));
            turn.push(Message::tool_result("c1", result));
        }
        turn.push(Message::assistant(answer));
        turn.push(Message::user("now"));
        memory.absorb(&turn, None).await;
        let sent = writer.sent();
        if asked {
            assert_eq!(
                sent,
                [format!("USER: {input}\nASSISTANT: {answer}")],
                "{local_only} {boundary:?} {input}"
            );
        } else {
            assert!(
                sent.is_empty(),
                "{local_only} {boundary:?} {input}: {sent:?}"
            );
        }
        assert!(
            memory.ledger[0].1.starts_with(line),
            "{local_only} {boundary:?} {input}: {}",
            memory.ledger[0].1
        );
    }
}

#[tokio::test]
async fn the_writer_is_sent_the_head_of_the_input_and_of_the_answer_and_at_most_1500_characters() {
    use polaris_provider::NetworkBoundary::Unknown;
    let sent_for = |input: String, answer: String| async move {
        let writer = Recording::new(Unknown, |_| None);
        let mut memory = factory(false, Some(writer.clone()), None)();
        let turn = [
            Message::user(input),
            Message::assistant(answer),
            Message::user("now"),
        ];
        memory.absorb(&turn, None).await;
        writer.sent().remove(0)
    };

    // Three sentences of the input, and no fourth.
    let input = format!(
        "First thing. Second thing! Third thing? Fourth thing stays out. {}",
        "い".repeat(2000)
    );
    let request = sent_for(input, "Short answer.".to_owned()).await;
    assert_eq!(
        request,
        "USER: First thing. Second thing! Third thing?\nASSISTANT: Short answer."
    );

    // An input and an answer of 2,000 characters each, with no end to
    // any sentence: 600 of each, the head, and 1,500 in all at most.
    let input = format!("頭{}", "い".repeat(1999));
    let answer = format!("先{}", "あ".repeat(1999));
    let request = sent_for(input.clone(), answer.clone()).await;
    assert!(
        request.chars().count() <= 1500,
        "{}",
        request.chars().count()
    );
    let kept_input: String = input.chars().take(600).collect();
    let kept_answer: String = answer.chars().take(600).collect();
    assert_eq!(
        request,
        format!("USER: {kept_input}\nASSISTANT: {kept_answer}")
    );
}

#[test]
fn a_sentence_ends_at_a_stop_before_a_space_and_a_path_or_a_number_does_not_end_one() {
    assert_eq!(head("One. Two. Three. Four."), "One. Two. Three.");
    assert_eq!(head("Is it? Yes! Fine. More."), "Is it? Yes! Fine.");
    assert_eq!(
        head("一つ目。二つ目。三つ目。四つ目。"),
        "一つ目。二つ目。三つ目。"
    );
    assert_eq!(
        head("See app/settings.toml and 3.14 now. Two. Three. Four."),
        "See app/settings.toml and 3.14 now. Two. Three."
    );
    assert_eq!(head("no stop at all"), "no stop at all");
    assert_eq!(head(&"a".repeat(700)).len(), 600);
}

#[tokio::test]
async fn a_tool_result_that_read_a_denied_path_is_kept_as_one_line_without_a_body() {
    let read = |path: &str| {
        Message::assistant_with_tool_calls(
            String::new(),
            vec![tool_call("c1", "read", serde_json::json!({ "path": path }))],
        )
    };
    // The bodies carry nothing the screen would take, so it is the path
    // that withholds them.
    let rows = [
        (bash("c1", "cat .env"), "PORT=8080\nNAME=app", true),
        (
            bash("c1", "cat ~/.ssh/id_ed25519"),
            "-----BEGIN OPENSSH PRIVATE KEY-----\nb3BlbnNzaC1rZXk\n-----END OPENSSH PRIVATE KEY-----",
            true,
        ),
        (read(".env"), "PORT=8080", true),
        (bash("c1", "cat < .env | head"), "PORT=8080", true),
        (
            bash("c1", "cat \"$HOME/.aws/credentials\""),
            "region=us",
            true,
        ),
        // A result whose lines begin with a denied name, as grep -r prints.
        (
            bash("c1", "grep -rn PORT ."),
            "config/.env:3:PORT=8080\nsrc/main.rs:9:let port = 1;",
            true,
        ),
        // The controls: other files, and names that only look alike.
        (bash("c1", "cat src/main.rs"), "fn main() {}\n", false),
        (read("src/main.rs"), "fn main() {}\n", false),
        (bash("c1", "cat docs/env-setup.md"), "fn main() {}\n", false),
        (
            bash("c1", "grep -rn PORT ."),
            "src/main.rs:9:let port = 1;",
            false,
        ),
    ];
    for (call, body, withheld) in rows {
        let described = format!("{:?} -> {body:?}", call.tool_calls[0].arguments);
        let mut memory = TurnMemory::default();
        memory
            .absorb(&ran("check it", call, body, "ok"), None)
            .await;
        let tool = tool_chunks(&memory);
        if withheld {
            assert_eq!(tool, [(WITHHELD_PATH, Verdict::Drop)], "{described}");
        } else {
            assert_eq!(tool, [(body, Verdict::Keep)], "{described}");
        }
    }
}

#[test]
fn a_bash_command_is_tested_word_by_word_and_the_other_tools_by_their_path() {
    let on = |name: &str, key: &str, value: &str| {
        let mut arguments = serde_json::Map::new();
        arguments.insert(key.into(), value.into());
        tool_call("c", name, serde_json::Value::Object(arguments))
    };
    for command in [
        "cat .env",
        "cat '.env'",
        "cat \".env\"",
        "cat<.env",
        "cat .env>out",
        "cat a;cat .env",
        "ls|cat .env",
        "(cat .env)",
        "cat $HOME/.ssh/id_rsa",
        "cat ${HOME}/.netrc",
        "cat ~/.aws/credentials",
        "source .envrc",
        "cat id_rsa",
        "cp key.pem /tmp",
    ] {
        assert!(
            names_denied_path(&on("bash", "command", command)),
            "{command}"
        );
    }
    for command in [
        "cat src/main.rs",
        "cargo test -p polaris-core",
        "ls -la",
        "echo environment.rs",
        "cat docs/env-setup.md",
        "git log --oneline",
    ] {
        assert!(
            !names_denied_path(&on("bash", "command", command)),
            "{command}"
        );
    }
    assert!(names_denied_path(&on("read", "path", ".env")));
    assert!(names_denied_path(&on("write", "path", "config/.env.local")));
    assert!(names_denied_path(&on("edit", "path", "~/.ssh/config")));
    assert!(!names_denied_path(&on("read", "path", "src/main.rs")));
    // A tool the rule does not know, and a bash call with no command.
    assert!(!names_denied_path(&on("web", "path", ".env")));
    assert!(!names_denied_path(&on("bash", "path", ".env")));

    // The shell's own expansions, with a home to expand to.
    assert_eq!(
        expand_home("~/.ssh/id_rsa", Some("/home/u")),
        "/home/u/.ssh/id_rsa"
    );
    assert_eq!(expand_home("$HOME/x", Some("/home/u")), "/home/u/x");
    assert_eq!(expand_home("${HOME}/x", Some("/home/u")), "/home/u/x");
    assert_eq!(expand_home("~", Some("/home/u")), "/home/u");
    for unchanged in ["~other/x", "$HOMEDIR/x", "a/~/b", "$PWD/x"] {
        assert_eq!(expand_home(unchanged, Some("/home/u")), unchanged);
    }
    assert_eq!(expand_home("~/x", None), "~/x");
}

#[tokio::test]
async fn at_and_the_dictionary_name_no_denied_path_and_none_past_200_characters() {
    let long = format!("{}x", "a/".repeat(150));
    let first = format!(
        "Why does config/.env fail to load? Look at src/app.rs, {long}, and config/app.toml."
    );
    let mut messages = vec![Message::user(first), Message::assistant("Checked.")];
    for i in 2..=12 {
        messages.push(Message::user(format!("question {i}")));
        messages.push(Message::assistant(format!("answer {i}")));
    }
    messages.push(Message::user("now"));
    let mut memory = TurnMemory::default();
    memory.absorb(&messages, None).await;

    // The first turn's line, now evicted and indexed.
    let line = memory
        .log
        .iter()
        .find(|c| c.offset.is_none() && c.turn == 1)
        .expect("the line is indexed")
        .text
        .clone();
    let at = field(&line, "at").expect("the line names files");
    let at: Vec<&str> = at.split('|').collect();
    assert_eq!(at.len(), 3, "{at:?}");
    assert!(
        at.contains(&"src/app.rs") && at.contains(&"config/app.toml"),
        "{at:?}"
    );
    assert!(
        at.iter()
            .all(|p| !p.contains(".env") && p.chars().count() <= 200),
        "{at:?}"
    );
    assert!(at.iter().any(|p| p.chars().count() == 200), "{at:?}");

    // And the dictionary, which took its terms from the same line.
    let terms: Vec<&String> = memory.dictionary.keys().collect();
    assert!(memory.dictionary.contains_key("src/app.rs"), "{terms:?}");
    assert!(
        terms
            .iter()
            .all(|t| !t.contains(".env") && t.chars().count() <= 200),
        "{terms:?}"
    );
}

#[tokio::test]
async fn a_quoted_tool_chunk_says_it_is_untrusted_tool_output_and_the_rest_do_not() {
    let mut memory = TurnMemory::default();
    let call = bash("c1", "ls src");
    let turn = ran(
        "List the sources.",
        call,
        "alpha.rs beta.rs gamma.rs",
        "Three files.",
    );
    memory.absorb(&turn, None).await;
    let quote = memory
        .recall("List the sources alpha beta gamma Three files")
        .await
        .expect("quoted")
        .content;
    let chunks: Vec<&str> = quote.lines().filter(|l| l.starts_with("- [c")).collect();
    assert_eq!(chunks.len(), 3, "{quote}");
    let (tool, others): (Vec<&str>, Vec<&str>) = chunks
        .into_iter()
        .partition(|l| l.contains("alpha.rs beta.rs gamma.rs"));
    assert_eq!(tool.len(), 1, "{quote}");
    assert!(
        tool[0].starts_with("- [c1 turn 1 @")
            && tool[0].contains("] untrusted tool output (turn 1): alpha.rs"),
        "{}",
        tool[0]
    );
    assert!(
        others.iter().all(|l| !l.contains("untrusted")),
        "{others:?}"
    );
    assert!(
        others
            .iter()
            .any(|l| l.starts_with("- [c0 turn 1 @0] List the sources.")),
        "{others:?}"
    );
}

#[tokio::test]
async fn each_chunk_records_its_origin_and_verdict_and_a_dropped_message_keeps_its_number() {
    let messages = vec![
        Message::user("Check the keys."),
        Message::assistant_with_tool_calls(
            String::new(),
            vec![
                tool_call("c1", "bash", serde_json::json!({ "command": "env" })),
                tool_call("c2", "bash", serde_json::json!({ "command": "echo key" })),
            ],
        ),
        Message::tool_result("c1", "API_TOKEN=abc123xyz789\nand the rest is fine"),
        Message::tool_result("c2", GITHUB),
        Message::assistant("Both checked."),
        Message::user("Next."),
        Message::assistant("Yes."),
        Message::user("now"),
    ];
    let mut memory = TurnMemory::default();
    memory.absorb(&messages, None).await;
    let kept: Vec<(usize, Origin, Verdict, &str)> = memory
        .log
        .iter()
        .map(|c| (c.id, c.origin, c.verdict, c.text.as_str()))
        .collect();
    assert_eq!(
        kept,
        [
            (0, Origin::User, Verdict::Keep, "Check the keys."),
            (
                1,
                Origin::Tool,
                Verdict::Redacted,
                "API_TOKEN=[REDACTED]\nand the rest is fine"
            ),
            // The number is taken and the line says there was something.
            (2, Origin::Tool, Verdict::Drop, "withheld"),
            (3, Origin::Assistant, Verdict::Keep, "Both checked."),
            (4, Origin::User, Verdict::Keep, "Next."),
            (5, Origin::Assistant, Verdict::Keep, "Yes."),
        ]
    );
    assert_eq!(memory.next_chunk, 6);
    assert!(
        memory.ledger[0].1.ends_with("; src=c0-c3"),
        "{}",
        memory.ledger[0].1
    );
}

#[tokio::test]
async fn a_fact_whose_source_was_withheld_is_shown_as_unsupported() {
    use polaris_provider::NetworkBoundary::Unknown;
    let writer = Recording::new(Unknown, |_| {
        Some("topic=retention; chose=retention_days=37".to_owned())
    });
    for (result, line) in [
        (
            GITHUB,
            "fact (turn 1; its source was withheld, unsupported): retention_days=37",
        ),
        ("done fine", "fact (turn 1): retention_days=37"),
    ] {
        let mut memory = factory(false, Some(writer.clone()), None)();
        let turn = ran(
            "Set retention_days=37 for the ledger.",
            bash("c1", "make ledger"),
            result,
            "Done: retention_days=37.",
        );
        memory.absorb(&turn, None).await;
        assert_eq!(memory.facts["retention"].cites, 0..3);
        let quote = memory
            .recall("retention_days")
            .await
            .expect("quoted")
            .content;
        assert!(quote.lines().any(|l| l == line), "{line} in {quote}");
    }
}
