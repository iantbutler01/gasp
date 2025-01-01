from gasp import WAILGenerator

prompt = r'''
object ParseNews {
    thoughts: String[]
    article: String
}

object AnswerQuestion {
    thoughts: String[]
    question: String
}

object RequestLatestArticles {
    thoughts: String[]
    category: String
}

union NewsTools = ParseNews | AnswerQuestion | RequestLatestArticles;

template NewsCaster(user_input: String) -> NewsTools {
    prompt: """
    You're an agent who helps people interface with the news you have access to three tools.

    The user has provided you with the following input: "{{user_input}}"

    ParseNews, AnswerQuestion and RequestLatestArticles.

    Do not output any text except the chosen tool. Thoughts can be provided in each tools "thought" argument.

    {{return_type}}
    """
}

main {
    template_args {
        user_input: String
    }

    let news_prompt = NewsCaster(user_input: $user_input);

    prompt {
        {{news_prompt}}
    }
}
'''

generator = WAILGenerator()
generator.load_wail(prompt)
warnings, errors = generator.validate_wail()
print(warnings)
print(errors)