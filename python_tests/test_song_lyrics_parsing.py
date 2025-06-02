import sys
import os
import time # Added time import
from typing import List, Union, Optional, Any
from gasp import Deserializable

# Add project root to sys.path to allow importing gasp
project_root = os.path.abspath(os.path.join(os.path.dirname(__file__), '..'))
if project_root not in sys.path:
    sys.path.insert(0, project_root)

from gasp import Parser

# --- Define the Python classes ---
# These are simplified versions. Ensure they match the structure gasp expects
# or how your application uses them. Gasp will try to instantiate these.

class Chat(Deserializable):
    content: str
    ref_id: Optional[int] = None 
    # _type_name is usually handled by gasp for union discrimination, 
    # not typically set as an attribute unless explicitly defined and used.

    def __init__(self, content: str, ref_id: Optional[int] = None, **kwargs: Any):
        self.content = content
        self.ref_id = ref_id
        # Allow additional fields from JSON to be ignored or handled if needed
        for key, value in kwargs.items():
            if key == "_type_name": # Store it if present in data
                setattr(self, key, value)
            # else:
            #     print(f"Chat ignoring unexpected field: {key}")


    def __repr__(self) -> str:
        ref_id_str = f", ref_id={self.ref_id}" if self.ref_id is not None else ""
        type_name_str = f", _type_name='{getattr(self, '_type_name', None)}'" if hasattr(self, '_type_name') else ""
        return f"Chat(content='{self.content[:30].replace(chr(10), ' ')}...'{ref_id_str}{type_name_str})"

class IssueForm(Deserializable):
    # Fields from your example
    provider: Optional[str] = None
    repo_or_team_id: Optional[str] = None
    title: Optional[str] = None
    description: Optional[str] = None
    labels: List[str] # No default, gasp should create empty list if missing or handle
    assignees: List[str] # No default
    bismuth_assigned: Optional[bool] = None
    status: Optional[str] = None
    priority: Optional[str] = None
    ref_id: Optional[int] = None

    # A flexible __init__ that allows partial instantiation by gasp
    def __init__(self, **kwargs: Any):
        self.labels = [] # Ensure list fields are initialized
        self.assignees = []

        for key, value in kwargs.items():
            setattr(self, key, value)

    def __repr__(self) -> str:
        parts = []
        
        # Helper to add part if attribute exists
        def add_part(attr_name, display_name=None, is_string_snippet=False, max_len=20):
            display_name = display_name or attr_name
            if hasattr(self, attr_name):
                val = getattr(self, attr_name)
                if is_string_snippet and isinstance(val, str):
                    snippet = val[:max_len].replace(chr(10), ' ').strip()
                    parts.append(f"{display_name}='{snippet}...'")
                elif isinstance(val, list):
                     parts.append(f"{display_name}={val}")
                elif val is not None:
                    parts.append(f"{display_name}='{val}'")
                else: # val is None
                    parts.append(f"{display_name}=None")
            # else: attribute doesn't exist yet, so don't show it.
        
        add_part('title')
        add_part('description', is_string_snippet=True)
        add_part('provider')
        add_part('repo_or_team_id', display_name='repo')
        add_part('status')
        add_part('priority')
        add_part('labels')
        add_part('assignees')
        
        if hasattr(self, 'bismuth_assigned') and getattr(self, 'bismuth_assigned') is not None:
            parts.append(f"bismuth_assigned={self.bismuth_assigned}")
            
        if hasattr(self, 'ref_id') and self.ref_id is not None:
            parts.append(f"ref_id={self.ref_id}")
            
        if hasattr(self, '_type_name'):
            type_name_val = getattr(self, '_type_name', None)
            if type_name_val is not None:
                 parts.append(f"_type_name='{type_name_val}'")

        return f"IssueForm({', '.join(parts)})"

AgentAction = Union[Chat, IssueForm]

# --- Main test function ---
def run_test_scenario(json_input_string: str, chunk_size: int = 10):
    print(f"\n--- Testing with chunk_size = {chunk_size} ---")
    parser = Parser(List[AgentAction])
    
    buffer = json_input_string
    total_fed = 0
    while buffer:
        chunk = buffer[:chunk_size]
        buffer = buffer[chunk_size:]
        total_fed += len(chunk)
        
        # print(f"\nFeeding chunk ({len(chunk)} chars, total_fed {total_fed}): '{chunk[:100].replace(chr(10), ' ')}...'")
        partial_result = parser.feed(chunk)
        if partial_result is None:
            print("Current Object: None")
        else:
            print("Current Object:")
            for item_idx, item in enumerate(partial_result):
                # ADD THIS LINE:
                print(f"  Item {item_idx} type: {type(item)}") 
                
                if hasattr(item, 'model_dump'):
                    print(f"  Item {item_idx}: {item.model_dump()}")
                else:
                    print(f"  Item {item_idx}: {item}") # Fallback to repr if no model_dump
        
        if parser.is_complete():
            print("Parser marked complete.")
        time.sleep(1) # Keep 1-second sleep, user can comment out if too slow
            
    print("\n--- Final validation (if parser not already complete) ---")
    if not parser.is_complete() and total_fed == len(json_input_string):
         # Try one last empty feed to finalize if not explicitly completed by close tag
        try:
            print("Feeding empty chunk to finalize...")
            partial_result = parser.feed("")
            print(f"Current Object after empty feed: {partial_result}")
        except Exception as e:
            print(f"Error during final empty feed: {e}")

    try:
        final_result = parser.validate() # validate() might be the same as get_partial() for now
        if final_result is None:
            print("Validated Object: None")
        else:
            print("Validated Object:")
            for item_idx, item in enumerate(final_result):
                if hasattr(item, 'model_dump'):
                    print(f"  Item {item_idx}: {item.model_dump()}")
                else:
                    print(f"  Item {item_idx}: {item}")
        print(f"Parser is_complete: {parser.is_complete()}")
    except Exception as e:
        print(f"Error during parser.validate: {e}")

    print("-" * 70)

if __name__ == "__main__":
    # IMPORTANT: Paste your full JSON string (including outer <List>...</List> tags)
    # into the triple-quoted string below.
    LLM_OUTPUT_STRING = r"""
Looking at this request, the user wants me to create a GitHub issue form. I should provide a conversational response and then create a comprehensive issue form.

<reference_id>
6409
</reference_id>

<List>
[
{
"_type_name": "Chat",
"content": "Hey! Absolutely, let me create a GitHub issue for you. I'll make something interesting about implementing a developer productivity analytics platform."
},
{
"_type_name": "IssueForm",
"provider": "github",
"repo_or_team_id": "analytics/dev-productivity",
"title": "Build comprehensive developer productivity analytics with AI-powered insights",
"description": "## Overview\nWe need a comprehensive analytics platform to understand developer productivity patterns, identify bottlenecks, and provide actionable insights to improve team performance and well-being.\n\n## Problem Statement\n- **Lack of Visibility**: No clear metrics on what impacts developer productivity\n- **Burnout Detection**: Can't identify early signs of developer burnout or overwork\n- **Process Inefficiencies**: Unknown bottlenecks in our development workflow\n- **Resource Allocation**: Poor understanding of where time is actually spent\n- **Team Optimization**: No data-driven approach to improve team dynamics\n\n## Proposed Solution\nDevelop an intelligent analytics platform that provides deep insights into developer productivity:\n\n### Core Analytics Features\n\n#### Individual Developer Insights\n- **Coding Patterns**: Track focus time, interruption frequency, and deep work sessions\n- **Skill Development**: Monitor learning progress and technology adoption\n- **Work-Life Balance**: Analyze work hours, weekend activity, and vacation usage\n- **Collaboration Metrics**: Measure code review participation and knowledge sharing\n- **Goal Tracking**: Personal OKRs and skill development objectives\n\n#### Team Performance Analytics\n- **Velocity Tracking**: Sprint completion rates and story point accuracy\n- **Collaboration Networks**: Visualize team communication and knowledge flow\n- **Bottleneck Identification**: Find process inefficiencies and blockers\n- **Code Quality Trends**: Track technical debt and refactoring efforts\n- **Meeting Efficiency**: Analyze meeting time and effectiveness\n\n#### Organizational Insights\n- **Engineering KPIs**: Lead time, deployment frequency, change failure rate\n- **Resource Utilization**: Understand capacity and workload distribution\n- **Retention Predictors**: Early warning signs for developer attrition\n- **ROI Analysis**: Measure impact of productivity initiatives\n- **Benchmarking**: Compare performance against industry standards\n\n### AI-Powered Features\n\n#### Predictive Analytics\n- **Burnout Prevention**: ML models to predict and prevent developer burnout\n- **Performance Forecasting**: Predict sprint outcomes and delivery timelines\n- **Skill Gap Analysis**: Identify training needs and career development opportunities\n- **Risk Assessment**: Flag projects at risk of delays or quality issues\n- **Optimal Team Composition**: Suggest team formations for maximum effectiveness\n\n#### Intelligent Recommendations\n- **Process Improvements**: Suggest workflow optimizations based on data patterns\n- **Focus Time Optimization**: Recommend ideal schedules for deep work\n- **Collaboration Enhancement**: Identify opportunities for knowledge sharing\n- **Tool Recommendations**: Suggest productivity tools based on usage patterns\n- **Career Guidance**: Personalized development path recommendations\n\n## Technical Architecture\n\n### Data Collection Layer\n- **IDE Integration**: VS Code, IntelliJ plugins for coding activity tracking\n- **Git Analytics**: Comprehensive analysis of commit patterns and code changes\n- **Calendar Integration**: Meeting analysis and focus time identification\n- **Communication Tools**: Slack, Teams integration for collaboration metrics\n- **Project Management**: Jira, GitHub integration for workflow analysis\n- **CI/CD Metrics**: Build times, deployment frequency, failure rates\n\n### Processing Pipeline\n- **Real-Time Streaming**: Kafka-based event processing for live insights\n- **Data Warehouse**: Snowflake for historical analysis and reporting\n- **ML Pipeline**: TensorFlow/PyTorch for predictive modeling\n- **Feature Engineering**: Automated feature extraction from raw data\n- **Privacy Engine**: Anonymization and consent management\n\n### Analytics Platform\n- **Interactive Dashboards**: React-based customizable analytics interface\n- **Mobile App**: iOS/Android app for on-the-go insights\n- **API Gateway**: GraphQL API for third-party integrations\n- **Notification System**: Smart alerts for important trends and anomalies\n- **Export Tools**: Data export for external analysis and reporting\n\n## Key Features\n\n### Privacy & Ethics First\n- **Transparent Data Collection**: Clear opt-in/opt-out for all metrics\n- **Individual Control**: Developers control their own data visibility\n- **Anonymized Insights**: Team and org-level insights without individual exposure\n- **GDPR Compliance**: Full compliance with privacy regulations\n- **Ethical AI**: Bias detection and fairness in all ML models\n\n### Developer Experience\n- **Personal Dashboard**: Individual productivity insights and goal tracking\n- **Focus Mode**: Distraction blocking and deep work optimization\n- **Learning Recommendations**: Personalized skill development suggestions\n- **Achievement System**: Gamification elements for motivation\n- **Peer Benchmarking**: Anonymous comparison with similar roles\n\n### Management Tools\n- **Team Health Monitoring**: Real-time team wellness and productivity metrics\n- **Resource Planning**: Data-driven capacity planning and allocation\n- **Performance Reviews**: Objective data to support review conversations\n- **Intervention Alerts**: Early warning system for team issues\n- **ROI Tracking**: Measure impact of productivity investments\n\n## Implementation Phases\n\n### Phase 1: Foundation (14 weeks)\n- Basic data collection from Git and project management tools\n- Simple analytics dashboard with core metrics\n- Privacy framework and consent management\n- Individual developer insights and personal dashboard\n- Basic team-level reporting\n\n### Phase 2: Intelligence (12 weeks)\n- ML model development for burnout prediction\n- Advanced analytics and trend identification\n- Intelligent recommendations engine\n- Mobile app development\n- Integration with communication tools\n\n### Phase 3: Advanced Features (10 weeks)\n- Predictive analytics for project outcomes\n- Advanced team optimization features\n- Comprehensive organizational insights\n- Third-party integrations and API\n- Advanced privacy and security features\n\n### Phase 4: Enterprise Scale (8 weeks)\n- Multi-tenant architecture for large organizations\n- Advanced compliance and audit features\n- Custom analytics and reporting\n- Enterprise integrations (SSO, LDAP)\n- Advanced ML models and insights\n\n## Success Metrics\n- **Adoption Rate**: 85%+ developer participation within 6 months\n- **Productivity Improvement**: 20% increase in meaningful productivity metrics\n- **Burnout Reduction**: 50% reduction in developer burnout incidents\n- **Process Optimization**: 30% improvement in development cycle times\n- **Developer Satisfaction**: 90%+ positive feedback on insights value\n- **Retention Impact**: 25% improvement in developer retention rates\n\n## Resource Requirements\n- **Engineering Team**: 8 engineers (backend, ML, frontend, mobile, DevOps)\n- **Data Scientists**: 3 specialists for analytics and ML model development\n- **UX/UI Designers**: 2 designers for dashboard and mobile app\n- **Privacy/Legal**: 1 specialist for compliance and ethics\n- **Infrastructure**: Cloud analytics platform, ML compute resources\n- **Timeline**: 12-18 months for full implementation\n\n## Risk Mitigation\n- **Privacy Concerns**: Transparent communication and strong privacy controls\n- **Developer Resistance**: Focus on individual value and voluntary participation\n- **Data Quality**: Robust validation and cleaning pipelines\n- **Bias in Analytics**: Regular bias audits and fairness testing\n- **Scalability**: Cloud-native architecture with horizontal scaling\n\n## Future Enhancements\n- **Cross-Company Benchmarking**: Anonymous industry comparisons\n- **Advanced AI Coaching**: Personalized productivity coaching\n- **Wellness Integration**: Mental health and well-being tracking\n- **Remote Work Optimization**: Specialized insights for distributed teams\n- **Code Quality Prediction**: AI models for predicting code quality issues",
"labels": ["epic", "analytics", "productivity", "ai", "developer-experience", "high-priority", "platform"],
"assignees": ["data-team", "ml-engineer", "product-manager", "ux-designer"],
"bismuth_assigned": true,
"status": "planning",
"priority": "high"
},
{
"_type_name": "Chat",
"content": "Perfect! Created a comprehensive GitHub issue for a developer productivity analytics platform with AI-powered insights. This one focuses on understanding developer patterns, preventing burnout, and optimizing team performance while maintaining strong privacy controls. The kind of platform that could really help teams work smarter, not harder! 📊🚀"
}
]
</List>
"""

    # Example of how to paste (replace the placeholder above):
    # LLM_OUTPUT_STRING = r"""<List>[{
    # "_type_name": "Chat",
    # "ref_id": 3901,
    # "content": "Here's a song for you:\n\n**\"Code and Dreams\"**\n\n*Verse 1:*\nSitting at my desk tonight, the screen glows bright and blue\nLines of code like poetry, each function something new\nDebugging through the errors, finding where they hide\nBuilding something beautiful, with logic as my guide\n\n*Chorus:*\nCode and dreams, they intertwine\nEvery bug becomes a sign\nThat we're closer to the goal\nWriting software for the soul\nCode and dreams, forever bound\nIn the silence, hear the sound\nOf progress being made\n\n*Verse 2:*\nFrom hello world to systems vast, we've come so far today\nCollaborating across the globe in our digital way\nOpen source and sharing knowledge, lifting each other high\nTechnology and human hearts reaching for the sky\n\n*Chorus:*\nCode and dreams, they intertwine\nEvery bug becomes a sign\nThat we're closer to the goal\nWriting software for the soul\nCode and dreams, forever bound\nIn the silence, hear the sound\nOf progress being made\n\n*Bridge:*\nWhen the compile finally works\nAnd the tests are turning green\nThat's the moment that we know\nWe've built something unforeseen\n\n*Final Chorus:*\nCode and dreams, they intertwine\nEvery bug becomes a sign\nThat we're closer to the goal\nWriting software for the soul"
    # }]</List>"""

    if "PASTE_YOUR_SONG_JSON_STRING_HERE" in LLM_OUTPUT_STRING or not LLM_OUTPUT_STRING.strip():
        print("ERROR: Please paste the JSON string into the LLM_OUTPUT_STRING variable in the script and re-run.")
        print("The string should be the full output from the LLM, including the <List>...</List> tags.")
    else:
        print(f"Input string length: {len(LLM_OUTPUT_STRING)}")
        # Test with various chunk sizes
        run_test_scenario(LLM_OUTPUT_STRING, chunk_size=50) 
        # run_test_scenario(LLM_OUTPUT_STRING, chunk_size=10) # Smaller chunks
        # run_test_scenario(LLM_OUTPUT_STRING, chunk_size=1)   # Stress test with 1-char chunks
        # run_test_scenario(LLM_OUTPUT_STRING, chunk_size=len(LLM_OUTPUT_STRING)) # Full string at once
