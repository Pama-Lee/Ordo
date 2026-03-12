FROM openpolicyagent/opa:1.4.2-static
COPY rules/opa/ /policies/
EXPOSE 8080
ENTRYPOINT ["opa"]
CMD ["run", "--server", "--addr", ":8080", "/policies/"]
