from typing import Optional, Any, Type, Dict, List, TypeVar, Generic, Union, ClassVar

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
    def __gasp_from_partial__(cls, partial_data: Dict[str, Any]) -> 'Deserializable':
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
    
    def __init__(self, type_obj: Optional[Type[T]] = None) -> None:
        """Initialize a parser for the given type"""
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
