# Jupyter Notebooks

Run these commands from this `jupyter` directory.

```powershell
.\.venv\Scripts\python.exe -m jupyter lab notebooks\water_flow_test\water_flow_test.ipynb
```

If the virtual environment needs to be recreated:

```powershell
python -m venv .venv
.\.venv\Scripts\python.exe -m pip install -r requirements.txt
```

For the Rust notebook, install the Evcxr kernel if it is not already available:

```powershell
cargo install evcxr_jupyter
evcxr_jupyter --install
```
