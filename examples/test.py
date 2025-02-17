from gasp import WAILGenerator
import json

gasp = WAILGenerator()

with open("./examples/prompt.wail") as f:
    prompt = f.read()

gasp.load_wail(prompt)

prompt, _, _ = gasp.get_prompt(files="./test.py", lines=200, pinned_files="./prompt.wail", viewer_state="test", turns=40, task="test")


print("============== Prompt ==============")
print("============== Prompt ==============")
print("============== Prompt ==============")
print("============== Prompt ==============")
print("============== Prompt ==============")
print(prompt)