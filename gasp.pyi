from typing import Dict, List, Optional, Tuple, Union
from typing_extensions import TypedDict

class Success(TypedDict):
    message: str

class Error(TypedDict):
    code: int
    details: str

class ValidationResult(TypedDict):
    error_type: str
    found: Optional[str]
    location: Optional[str]
    expected: Optional[str]
    name: Optional[str]
    template_name: Optional[str]
    reason: Optional[str]

class WAILGenerator:
    def __init__(self, base_dir=None) -> None: ...

    def set_base_dir(self, base_dir: str) -> None: ...
    
    def load_wail(self, content: str) -> Optional[ValidationResult]: ...
    
    def get_prompt(self, **kwargs: Union[str, int, float]) -> Tuple[Optional[str], List[str], List[str]]: ...
    
    def parse_llm_output(self, llm_output: str) -> Union[Dict, List]: ...
    
    def validate_wail(self) -> Tuple[List[str], List[str]]: ...