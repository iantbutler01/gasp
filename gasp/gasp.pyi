from typing import Optional, Any, Type, Dict, List, TypeVar, Generic, Union, ClassVar
import jinja2

T = TypeVar('T')

class Deserializable:
    """Base class for types that can be deserialized from JSON"""
    __gasp_fields__: ClassVar[Dict[str, Any]]
    __gasp_annotations__: ClassVar[Dict[str, Any]]
    
    @classmethod
    def __gasp_register__(cls) -> None:
        """Register the type for deserialization"""
        pass
    
    @classmethod
    def __gasp_from_partial__(cls: Type[T], partial_data: Dict[str, Any]) -> T:
        """Create an instance from partial data"""
        pass
    
    def __gasp_update__(self, new_data: Dict[str, Any]) -> None:
        """Update instance with new data"""
        pass
    
    # Pydantic V2 compatibility methods
    @classmethod
    def model_validate(cls: Type[T], obj: Dict[str, Any]) -> T:
        """Pydantic V2 compatible validation method"""
        pass
    
    @classmethod
    def model_fields(cls) -> Dict[str, Any]:
        """Return field information compatible with Pydantic V2"""
        pass
    
    def model_dump(self) -> Dict[str, Any]:
        """Convert model to dict (Pydantic V2 compatible)"""
        pass

class Parser(Generic[T]):
    """Parser for incrementally building typed objects from JSON streams"""
    
    def __init__(self, type_obj: Optional[Type[T]] = None, ignored_tags: Optional[List[str]] = None) -> None:
        """
        Initialize a parser for the given type.
        
        Args:
            type_obj: The Python type to parse into
            ignored_tags: List of tag names to ignore. Defaults to ["think", "thinking", "system"]
        """
        pass
    
    @staticmethod
    def from_pydantic(pydantic_model: Any) -> 'Parser':
        """Create a parser for a Pydantic model"""
        pass
    
    def feed(self, chunk: str) -> Optional[T]:
        """Feed a chunk of JSON data and return a partial object if available"""
        pass
    
    def is_complete(self) -> bool:
        """Check if parsing is complete"""
        pass
    
    def get_partial(self) -> Optional[T]:
        """Get the current partial object without validation"""
        pass
    
    def validate(self) -> Optional[T]:
        """Perform full validation on the completed object"""
        pass

class StreamParser:
    """Low-level streaming JSON parser"""
    
    def __init__(self) -> None:
        """Initialize a streaming parser"""
        pass
    
    def parse(self, chunk: str) -> Optional[Any]:
        """Feed a chunk of JSON data and return parsed value if complete"""
        pass
    
    def is_done(self) -> bool:
        """Check if parsing is complete"""
        pass

# Template helper functions
def type_to_format_instructions(type_obj: Type, name: Optional[str] = None) -> str:
    """
    Generate format instructions for a Python type.
    
    Args:
        type_obj: The Python type to generate instructions for
        name: Optional name to use for the type tag (defaults to class name)
        
    Returns:
        A string containing format instructions
    """
    pass

def interpolate_prompt(template: str, type_obj: Type, format_tag: str = "return_type", name: Optional[str] = None) -> str:
    """
    Replace {{format_tag}} in the template with format instructions for the type.
    
    Args:
        template: The prompt template with {{format_tag}} placeholders
        type_obj: The Python type to generate instructions for
        format_tag: The tag to replace (default: "return_type")
        name: Optional name to use for the type tag (defaults to class name)
        
    Returns:
        The interpolated prompt
    """
    pass

# Jinja2 helper functions
def create_type_environment() -> jinja2.Environment:
    """
    Create a Jinja2 environment with GASP type formatting filters.
    
    Returns:
        A Jinja2 Environment with GASP custom filters.
    """
    pass

def format_type_filter(type_obj: Type, name: Optional[str] = None) -> str:
    """
    Jinja2 filter for formatting a type as instructions.
    
    Args:
        type_obj: The Python type to format
        name: Optional name override for the type tag
        
    Returns:
        Formatted type instructions
    """
    pass

def type_description_filter(type_obj: Type) -> str:
    """
    Jinja2 filter that returns a simple text description of a type.
    
    Args:
        type_obj: The Python type to describe
        
    Returns:
        Human-readable description of the type
    """
    pass

def render_template(template_str: str, context: Dict[str, Any], env: Optional[jinja2.Environment] = None) -> str:
    """
    Render a Jinja2 template with the given context.
    
    Args:
        template_str: Jinja2 template string
        context: Dictionary of variables to use in the template
        env: Optional Jinja2 environment (creates one with GASP filters if not provided)
        
    Returns:
        The rendered template as a string
    """
    pass

def render_file_template(template_path: str, context: Dict[str, Any], env: Optional[jinja2.Environment] = None) -> str:
    """
    Render a Jinja2 template file with the given context.
    
    Args:
        template_path: Path to the Jinja2 template file
        context: Dictionary of variables to use in the template
        env: Optional Jinja2 environment (creates one with GASP filters if not provided)
        
    Returns:
        The rendered template as a string
    """
    pass

# Module exports
template_helpers: Any
jinja_helpers: Any
